//! CLI：参数解析 + 三个子命令（submit / list / show）。
//!
//! 解析只用 std（不引 clap——硬纪律：默认拒绝新依赖）。用法错误 = OR1001、
//! 退出码 2；其余失败退出码 1，stderr 带 OR/EX 稳定码与原因。
//!
//! submit 的生命周期编排全部委托 af-execline 的 `OrderLifecycleDriver`
//! （幂等键重放、ledger、MES 落盘），门控判定走其 `EngineClient` 拉起的真
//! af-engine 子进程——本 crate 不重造这些轮子，只做门面。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use af_execline::{
    draft_fn_real_ring, AttemptLayout, DriverConfig, EngineClient, HarnessRunner, IngestFetcher,
    MesChSink, MesChTarget, MesFileWriter, OrderLifecycleDriver, SshBridge, StubExecuteUnit,
    SwevExecuteUnit, SystemClock,
};
use af_workorder::{Priority, Sla, StableId};
use serde_json::{json, Value};

use crate::error::OrderError;
use crate::exec::{
    graph_unit_for, parse_exec_flags, parse_unit_choice, report_unit_for, resolve_unit,
    swev_unit_spec_for, RealExec, SelectedUnit, UnitChoice, DEFAULT_HOST, EXEC_FLAGS,
};
use crate::spec::parse_spec;
use crate::state::{self, StateDir};
use crate::voucher;

/// 骨架默认图版本（WorkOrder 创建时一次性绑定；重放由账本快照恢复，不再读它）。
pub const DEFAULT_GRAPH_VERSION: &str = "af-order/poc-1";
/// MES 事件的 station_id（骨架固定值）。
pub const DEFAULT_STATION_ID: &str = "af-order-cli";

/// 解析后的子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// 提交订单：一命令走通全生命周期（幂等，重复 submit 只重放）。
    /// `exec` = 真实执行参数面（None = stub 冒烟档；见 [`crate::exec`]）；
    /// `mes_ch` = MES CH 目标声明（记账面，与执行面正交——冒烟档同样可声明）。
    Submit {
        spec: PathBuf,
        state: PathBuf,
        workspace: Option<PathBuf>,
        target_dir: Option<PathBuf>,
        /// 真实执行参数（消债 B① 正式契约；None = 冒烟档）。
        exec: Option<RealExec>,
        /// 单元路由（裁决7：swev|report|stub；Auto=缺省现行为）。
        unit: UnitChoice,
        /// MES CH 目标（"auto" 或显式 URL；None = 只落文件正本）。
        mes_ch: Option<String>,
    },
    /// 列出全部订单及终态。
    List { state: PathBuf },
    /// 打印单订单 ledger 摘要与 MES 事件。
    Show { order_id: String, state: PathBuf },
}

/// 执行一次 CLI。`args` 不含程序名；Ok = 应打到 stdout 的文本。
pub fn run(args: &[String]) -> Result<String, OrderError> {
    match parse_args(args)? {
        Command::Submit {
            spec,
            state,
            workspace,
            target_dir,
            exec,
            unit,
            mes_ch,
        } => run_submit(
            &spec,
            &state,
            workspace.as_deref(),
            target_dir.as_deref(),
            exec.as_ref(),
            unit,
            mes_ch.as_deref(),
        ),
        Command::List { state } => run_list(&state),
        Command::Show { order_id, state } => run_show(&order_id, &state),
    }
}

/// 解析命令行参数。
pub fn parse_args(args: &[String]) -> Result<Command, OrderError> {
    let Some((sub, rest)) = args.split_first() else {
        return Err(OrderError::Usage {
            detail: "缺少子命令".into(),
        });
    };
    match sub.as_str() {
        "submit" => {
            let mut allowed: Vec<&str> = vec!["--spec", "--state", "--workspace", "--target-dir"];
            allowed.extend_from_slice(EXEC_FLAGS);
            allowed.push("--unit"); // 裁决7 单元路由（缺省现行为）
            allowed.push("--mes-ch");
            let (flags, positional) = split_flags(rest, &allowed)?;
            forbid_positional(&positional, "submit")?;
            // 执行面三件套规则与缺省见 crate::exec（B① 正式契约）；
            // --mes-ch 是记账声明（与执行面正交），原样透传驱动缝。
            let exec = parse_exec_flags(&flags)?;
            Ok(Command::Submit {
                spec: PathBuf::from(require_flag(&flags, "--spec")?),
                state: PathBuf::from(require_flag(&flags, "--state")?),
                workspace: flag_value(&flags, "--workspace").map(PathBuf::from),
                target_dir: flag_value(&flags, "--target-dir").map(PathBuf::from),
                exec,
                unit: match flag_value(&flags, "--unit") {
                    Some(raw) => parse_unit_choice(raw)?,
                    None => UnitChoice::Auto,
                },
                mes_ch: flag_value(&flags, "--mes-ch").map(str::to_string),
            })
        }
        "list" => {
            let (flags, positional) = split_flags(rest, &["--state"])?;
            forbid_positional(&positional, "list")?;
            Ok(Command::List {
                state: PathBuf::from(require_flag(&flags, "--state")?),
            })
        }
        "show" => {
            let (flags, mut positional) = split_flags(rest, &["--state"])?;
            if positional.len() != 1 {
                return Err(OrderError::Usage {
                    detail: format!(
                        "show 恰好接一个 <order-id>（收到 {} 个位置参数）",
                        positional.len()
                    ),
                });
            }
            let order_id = positional.remove(0).to_string();
            Ok(Command::Show {
                order_id,
                state: PathBuf::from(require_flag(&flags, "--state")?),
            })
        }
        other => Err(OrderError::Usage {
            detail: format!("未知子命令 {other}（可选 submit/list/show）"),
        }),
    }
}

/// `--flag value` 拆分结果：flag（全名含 `--` → 值）有序表 + 位置参数。
type SplitFlags<'a> = (Vec<(&'a str, String)>, Vec<&'a str>);

/// 把 `--flag value` 拆进 flags、其余归位置参数。未知 flag / 缺值 / 同名
/// 旗标重复 = 用法错误。flag 键存全名（含 `--`），与 `require_flag` /
/// `flag_value` 的查询名一致。
fn split_flags<'a>(args: &'a [String], allowed: &[&str]) -> Result<SplitFlags<'a>, OrderError> {
    let mut flags = Vec::new();
    let mut positional = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with("--") {
            if !allowed.contains(&arg) {
                return Err(OrderError::Usage {
                    detail: format!("未知参数 {arg}"),
                });
            }
            let value = args.get(index + 1).ok_or_else(|| OrderError::Usage {
                detail: format!("{arg} 需要一个值"),
            })?;
            if value.starts_with("--") {
                return Err(OrderError::Usage {
                    detail: format!("{arg} 的值不能以 -- 开头: {value}"),
                });
            }
            flags.push((arg, value.clone()));
            index += 2;
        } else {
            positional.push(arg);
            index += 1;
        }
    }
    // 红队 W2-C36：同名旗标重复出现此前被静默取首个（flag_value 的 find），
    // 后续值无声丢弃——--state 这类决定账本落点的旗标静默取错目录尤其危险。
    // 重复 = OR1001 fail-loud，一次点名全部重复旗标。
    let mut duplicated: Vec<&str> = Vec::new();
    for (key, _) in &flags {
        let count = flags.iter().filter(|(other, _)| *other == *key).count();
        if count > 1 && !duplicated.contains(key) {
            duplicated.push(key);
        }
    }
    if !duplicated.is_empty() {
        duplicated.sort_unstable();
        return Err(OrderError::Usage {
            detail: format!(
                "参数重复出现: {}（同名旗标只允许一次；重复值此前会被静默丢弃）",
                duplicated.join(" ")
            ),
        });
    }
    Ok((flags, positional))
}

/// 取旗标值（split_flags 已保证同名旗标不重复，find 即唯一值）。
fn flag_value<'a>(flags: &'a [(&'a str, String)], name: &str) -> Option<&'a str> {
    flags
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

fn require_flag(flags: &[(&str, String)], name: &str) -> Result<String, OrderError> {
    flag_value(flags, name)
        .map(str::to_string)
        .ok_or_else(|| OrderError::Usage {
            detail: format!("缺少 {name}"),
        })
}

fn forbid_positional(positional: &[&str], sub: &str) -> Result<(), OrderError> {
    if let Some(first) = positional.first() {
        return Err(OrderError::Usage {
            detail: format!("{sub} 不接受位置参数: {first}"),
        });
    }
    Ok(())
}

// ---------- submit ----------

fn run_submit(
    spec_path: &Path,
    state_root: &Path,
    workspace: Option<&Path>,
    target_dir: Option<&Path>,
    exec: Option<&RealExec>,
    unit: UnitChoice,
    mes_ch: Option<&str>,
) -> Result<String, OrderError> {
    let text = fs::read_to_string(spec_path).map_err(|err| OrderError::SpecRead {
        path: spec_path.display().to_string(),
        detail: err.to_string(),
    })?;
    let mut spec = parse_spec(&text)?;

    // 红队批（W1）三件先行计算（均在凭证签发前——凭证/订单/MES 三面同值）：
    // ① 单元路由（stub 判别）；② 任务包判据面 sha256（W1-C59）；③ 计划
    //    graph_version（W1-C32：门面按计划图版本取值，统一值域）。
    let selected = resolve_unit(unit, exec)?;
    let task_digest = pin_task_digest(&selected, exec);
    let plan_digest = pin_plan_digest(&selected, exec);
    let plan_graph_version = read_plan_graph_version(&selected, exec)
        .unwrap_or_else(|| DEFAULT_GRAPH_VERSION.to_string());
    // W1-C64：stub 冒烟档 MES line_id 加 stub: 前缀——冒烟单与真单此前在
    // MES 账面逐字段不可区分（同 kind/quantity/station/line），下游
    // mes_line_hour_mv 按线聚合会把冒烟产量计入产线；前缀即判别量，
    // 产线聚合口径按前缀排除。凭证/订单绑定/MES 三面用同一前缀值。
    let line_binding = match selected {
        crate::exec::SelectedUnit::Stub => af_workorder::LineBinding {
            line_id: format!("stub:{}", spec.line.line_id),
            line_version: spec.line.line_version.clone(),
        },
        _ => spec.line.clone(),
    };
    // 凭证面 line 钉死（W1-C41）与 MES/订单面用同一值：冒烟档即 stub: 前缀。
    spec.line = line_binding.clone();

    // 状态目录与订单目录（order_id 已过路径安全校验）
    let state = StateDir::new(state_root);
    let order_dir = state.order_dir(&spec.order_id);
    fs::create_dir_all(&order_dir).map_err(|err| OrderError::State {
        detail: format!("创建 {}: {err}", order_dir.display()),
    })?;

    // 签发预算凭证（ed25519；重复 submit 读回原凭证，不重签）
    let stable_id = StableId::generate(&spec.order_id);
    let (voucher, voucher_record) =
        voucher::issue_or_load(&order_dir, &spec, &stable_id, plan_digest.as_deref())?;

    // 真引擎子进程客户端（定位或构建 af-engine；EX1001/EX1002 fail-loud）
    let (workspace, target_dir) = resolve_build_dirs(workspace, target_dir);
    let client = EngineClient::locate(&workspace, &target_dir)?;

    // 全生命周期驱动（复用 af-execline；同 order_id 重放不重复终态）。
    // 执行面透传（B①）：真实路径从 RealExec 注入 P4/P5 字段；冒烟档填中性
    // 占位（stub 单元不消费这些字段）。
    let cfg = DriverConfig {
        order_id: spec.order_id.clone(),
        // W1-C32：门面按任务包计划图版本取值（graph/report 档 = 计划
        // compiled_from.graph_version；无计划档回落骨架缺省）——与
        // planbind 装订期待同一值域，一次性绑定真核对。
        graph_version: plan_graph_version.clone(),
        sla: Sla::new(None, Priority::Normal),
        line: line_binding,
        task_sha256: task_digest,
        plan_sha256: plan_digest,
        voucher,
        out_dir: order_dir.clone(),
        station_id: DEFAULT_STATION_ID.to_string(),
        host_alias: exec
            .map(|e| e.host.clone())
            .unwrap_or_else(|| DEFAULT_HOST.into()),
        data_root: exec.map(|e| e.data_root.clone()).unwrap_or_default(),
        task_id: exec.map(|e| e.task.clone()).unwrap_or_default(),
        trial: false,
        profile: exec
            .map(|e| e.profile.clone())
            .unwrap_or_else(|| "stub".into()),
        attempt_layout: match unit {
            // 图执行器：graph- 前缀 D-20 递增布局（套件面区分）。
            UnitChoice::Graph => AttemptLayout::GraphIncrement,
            _ => AttemptLayout::SwevIncrement,
        },
    };
    // MES 写缝（B②③）：--mes-ch 只是目标声明（与执行面正交），写侧
    // （含读侧对账）全在 af-execline 的 MesChSink；不声明 = 文件正本。
    let mes_ch_target = mes_ch.map(MesChTarget::parse).transpose()?;
    let mut driver = match &mes_ch_target {
        Some(target) => {
            let sink = MesChSink::open(target, &order_dir).map_err(OrderError::from)?;
            OrderLifecycleDriver::open_with_sink(cfg, client, Box::new(SystemClock), Box::new(sink))
        }
        None => OrderLifecycleDriver::open(cfg, client, Box::new(SystemClock)),
    }?;
    // 执行单元：真实路径 = SwevExecuteUnit（SshBridge + HarnessRunner 真模型
    // 调用环 + 冻结判分指纹门）；冒烟档 = StubExecuteUnit（明示非生产路径）。
    // 预算硬顶由执行面注入（B①：--max-turns/--spend-cap-cents 覆写 fix2 缺省）。
    // 单元路由（裁决7：--unit swev|report|graph|stub；缺省=现行为）。报告臂装载期
    // 即过 planbind 三重门（指纹/模板/档案）+ 任务包材料核对（fail-loud）；
    // 图臂 = 图执行器（planbind 四核对 + 拓扑序逐节点驱动）。
    let report = match selected {
        SelectedUnit::Swev => {
            let exec = exec.expect("resolve 已保证 Swev 携三件套");
            let unit_spec = swev_unit_spec_for(exec);
            let runner = HarnessRunner::new().map_err(|err| OrderError::State {
                detail: format!("HarnessRunner 装载失败: {err}"),
            })?;
            let unit = SwevExecuteUnit::new(unit_spec, Arc::new(SshBridge), Arc::new(runner))
                .map_err(OrderError::from)?;
            driver.drive(&unit)?
        }
        SelectedUnit::Report => {
            let exec = exec.expect("resolve 已保证 Report 携三件套");
            let unit_spec = report_unit_for(exec)?;
            // F-第五单：真模型环接线（reader→writer 双节点 + cite_check 反馈环）；
            // 摄取分派器按 source 形状选路（https/arXiv web 面、本地文本/PDF 面）。
            let runner = HarnessRunner::new().map_err(|err| OrderError::State {
                detail: format!("HarnessRunner 装载失败: {err}"),
            })?;
            let unit = af_execline::ReportExecuteUnit::new(
                unit_spec.clone(),
                draft_fn_real_ring(Arc::new(runner), unit_spec),
                Arc::new(IngestFetcher),
            )
            .map_err(OrderError::from)?;
            driver.drive(&unit)?
        }
        SelectedUnit::Graph => {
            // 图执行器（F-第四单执行长缺口销项）：planbind 四核对 + 拓扑序逐
            // 节点驱动；计划从任务包约定路径 <data_root>/<task>/plan.json 读。
            let exec = exec.expect("resolve 已保证 Graph 携三件套");
            let unit_spec = graph_unit_for(exec)?;
            let runner = HarnessRunner::new().map_err(|err| OrderError::State {
                detail: format!("HarnessRunner 装载失败: {err}"),
            })?;
            let unit = af_execline::GraphExecuteUnit::new(
                unit_spec,
                Arc::new(SshBridge),
                Arc::new(runner),
            )
            .map_err(OrderError::from)?;
            driver.drive(&unit)?
        }
        SelectedUnit::Stub => {
            let artifact_id = format!("fa-{}", spec.order_id);
            driver.drive(&StubExecuteUnit::qualified_final(artifact_id.clone()))?
        }
    };
    // drive 收尾已含 MES 对账面（CH sink：读侧 event_id 对账 + 差量送批；
    // Parked = EX2003 fail-loud 透传）。
    let mes_sync = &report.mes_sync;

    let verdict = report.exec_output.as_ref().and_then(|out| out.verdict);
    let exec_record = report
        .exec_output
        .as_ref()
        .and_then(|out| out.record.clone());
    let evidence = report
        .exec_output
        .as_ref()
        .map(|out| {
            out.evidence
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let summary = json!({
        "order_id": spec.order_id,
        "stable_id": stable_id.as_str(),
        "state": state::state_name(report.order.state()),
        "replayed": report.replayed,
        "voucher_id": voucher_record.voucher_id,
        "voucher_key_id": voucher_record.key_id,
        "exec": exec.map(|e| json!({
            // 单元名如实（--unit 路由产物；Auto 缺省 = swev）。
            "unit": match selected {
                SelectedUnit::Swev => "swev-unit",
                SelectedUnit::Report => "report-unit",
                SelectedUnit::Graph => "graph-unit",
                SelectedUnit::Stub => "stub-unit",
            },
            "task": e.task,
            "profile": e.profile,
            "host": e.host,
            // 红队 W2-C33：如实回显声明面——null = 用户未给旗标（各单元
            // 自载缺省生效），不把 CLI 缺省值冒充用户预算。
            "max_turns": e.max_turns,
            "spend_cap_cents": e.spend_cap_cents,
        })),
        "verdict": verdict,
        "record": exec_record,
        "evidence": evidence,
        "mes_ch": mes_ch_target.as_ref().map(|_| serde_json::to_value(mes_sync).unwrap_or_default()),
        "ledger_ops_appended": report.appended,
        "mes_events_emitted": report.mes_events.len(),
        "order_dir": order_dir.display().to_string(),
        "engine_bin": driver.engine_bin().display().to_string(),
    });
    to_pretty_json(&summary)
}

/// 解析 workspace 根与构建目录（优先级：显式参数 > 环境变量 > 默认）。
///
/// workspace：`--workspace` > `AF_WORKSPACE_ROOT` > `.`（cwd）。
/// target：`--target-dir` > `CARGO_TARGET_DIR` > `<workspace>/target`。
fn resolve_build_dirs(workspace: Option<&Path>, target_dir: Option<&Path>) -> (PathBuf, PathBuf) {
    resolve_build_dirs_with(
        workspace,
        target_dir,
        std::env::var_os("AF_WORKSPACE_ROOT").map(PathBuf::from),
        std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
    )
}

/// 纯函数形态（env 值注入，可单测；并行测试下直接改环境变量有竞态）：
/// 显式参数 > env > 默认。
fn resolve_build_dirs_with(
    workspace: Option<&Path>,
    target_dir: Option<&Path>,
    env_workspace: Option<PathBuf>,
    env_target_dir: Option<PathBuf>,
) -> (PathBuf, PathBuf) {
    let workspace = workspace
        .map(PathBuf::from)
        .or(env_workspace)
        .unwrap_or_else(|| PathBuf::from("."));
    let target_dir = target_dir
        .map(PathBuf::from)
        .or(env_target_dir)
        .unwrap_or_else(|| workspace.join("target"));
    (workspace, target_dir)
}

// ---------- list ----------

fn run_list(state_root: &Path) -> Result<String, OrderError> {
    let summaries = StateDir::new(state_root).list()?;
    let lines: Vec<String> = summaries
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()
        .map_err(|err| OrderError::State {
            detail: format!("清单序列化失败: {err}"),
        })?;
    Ok(lines.join("\n"))
}

// ---------- show ----------

fn run_show(order_id: &str, state_root: &Path) -> Result<String, OrderError> {
    // 红队 W2-C39：order_id 位置参数与 submit 共用同一路径安全门——绝对
    // 路径（Path::join 会替换 state 根）、分隔符、`..` 段在此拒尽，不得
    // 读出 state/orders/ 之外的账本。
    crate::spec::validate_order_id(order_id).map_err(|err| OrderError::Usage {
        detail: format!("order_id 不当作目录名（{err}）"),
    })?;
    let order_dir = StateDir::new(state_root).order_dir(order_id);
    let ledger_path = order_dir.join(state::LEDGER_FILE);
    if !ledger_path.is_file() {
        return Err(OrderError::OrderNotFound {
            order_id: order_id.to_string(),
        });
    }
    let records = state::load_ledger(&ledger_path)?;
    let Some(first) = records.first() else {
        return Err(OrderError::OrderNotFound {
            order_id: order_id.to_string(),
        });
    };

    let ops: Vec<Value> = records
        .iter()
        .map(|record| {
            json!({
                "seq": record.seq,
                "ts_ms": record.ts_ms,
                "op": record.op,
                "ok": record.ok,
                "state_after": record.state_after,
                "error_kind": record.error_kind,
                "error_detail": record.error_detail,
            })
        })
        .collect();

    // MES 事件：无文件 = 空列表（订单未走到任何跃迁时可能没有）；有则坏行 OR2005。
    let mes_path = order_dir.join(state::MES_FILE);
    let events: Vec<Value> = if mes_path.is_file() {
        MesFileWriter::load(&mes_path)
            .map_err(|err| OrderError::State {
                detail: err.to_string(),
            })?
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<_, _>>()
            .map_err(|err| OrderError::State {
                detail: format!("MES 事件序列化失败: {err}"),
            })?
    } else {
        Vec::new()
    };

    let out = json!({
        "order_id": first.order_id,
        "stable_id": first.stable_id,
        "state": state::current_state(&records).unwrap_or_else(|| "Unknown".into()),
        "ledger": { "path": ledger_path.display().to_string(), "ops": ops },
        "mes": { "path": mes_path.display().to_string(), "events": events },
    });
    to_pretty_json(&out)
}

fn to_pretty_json(value: &Value) -> Result<String, OrderError> {
    serde_json::to_string_pretty(value).map_err(|err| OrderError::State {
        detail: format!("输出序列化失败: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 优先级全覆盖：显式参数 > env > 默认（env 以注入值断言，不真改环境）。
    #[test]
    fn resolve_build_dirs_prefers_explicit_args_then_env_then_defaults() {
        // 显式参数双全
        let (ws, td) = resolve_build_dirs_with(
            Some(Path::new("/ws")),
            Some(Path::new("/td")),
            Some(PathBuf::from("/env-ws")),
            Some(PathBuf::from("/env-td")),
        );
        assert_eq!(
            (ws.as_path(), td.as_path()),
            (Path::new("/ws"), Path::new("/td"))
        );

        // workspace 显式、target 走 env
        let (ws, td) = resolve_build_dirs_with(
            Some(Path::new("/ws")),
            None,
            Some(PathBuf::from("/env-ws")),
            Some(PathBuf::from("/env-td")),
        );
        assert_eq!(
            (ws.as_path(), td.as_path()),
            (Path::new("/ws"), Path::new("/env-td"))
        );

        // 全靠 env workspace，target 落 env workspace 下
        let (ws, td) = resolve_build_dirs_with(None, None, Some(PathBuf::from("/env-ws")), None);
        assert_eq!(
            (ws.as_path(), td.as_path()),
            (Path::new("/env-ws"), Path::new("/env-ws/target"))
        );

        // 什么都没有：cwd + <cwd>/target
        let (ws, td) = resolve_build_dirs_with(None, None, None, None);
        assert_eq!(
            (ws.as_path(), td.as_path()),
            (Path::new("."), Path::new("./target"))
        );
    }

    /// 参数解析：三个子命令的形状与用法错误。
    #[test]
    fn parse_args_shapes_and_usage_errors() {
        let args = |parts: &[&str]| -> Vec<String> {
            parts.iter().map(|part| (*part).to_string()).collect()
        };

        let submit = parse_args(&args(&["submit", "--spec", "o.json", "--state", "st"])).unwrap();
        assert_eq!(
            submit,
            Command::Submit {
                spec: PathBuf::from("o.json"),
                state: PathBuf::from("st"),
                workspace: None,
                target_dir: None,
                // 消债 B①：Submit 增 exec 执行面（None = stub 冒烟档）与
                // mes_ch 记账声明（正交）；裁决7：unit 单元路由缺省 Auto
                exec: None,
                unit: UnitChoice::Auto,
                mes_ch: None,
            }
        );
        let Command::Submit {
            workspace,
            target_dir,
            ..
        } = parse_args(&args(&[
            "submit",
            "--spec",
            "o.json",
            "--state",
            "st",
            "--workspace",
            "/ws",
            "--target-dir",
            "/td",
        ]))
        .unwrap()
        else {
            panic!("应解析为 Submit");
        };
        assert_eq!(workspace.as_deref(), Some(Path::new("/ws")));
        assert_eq!(target_dir.as_deref(), Some(Path::new("/td")));

        assert_eq!(
            parse_args(&args(&["list", "--state", "st"])).unwrap(),
            Command::List {
                state: PathBuf::from("st")
            }
        );
        assert_eq!(
            parse_args(&args(&["show", "wo-1", "--state", "st"])).unwrap(),
            Command::Show {
                order_id: "wo-1".into(),
                state: PathBuf::from("st")
            }
        );

        // 用法错误全部 OR1001 / 退出码 2
        for bad in [
            Vec::new(),
            args(&["bogus"]),
            args(&["submit", "--spec", "o.json"]), // 缺 --state
            args(&["submit", "--spec", "o.json", "--state"]), // --state 缺值
            args(&["submit", "--spoke", "x", "--state", "s"]), // 未知 flag
            args(&["list", "--state", "st", "extra"]), // list 不收位置参数
            args(&["show", "--state", "st"]),      // show 缺 order-id
        ] {
            let err = parse_args(&bad).unwrap_err();
            assert!(matches!(err, OrderError::Usage { .. }), "{err}");
            assert_eq!(err.exit_code(), 2, "{err}");
        }
    }

    /// W2-C36：同名旗标重复出现 = OR1001 用法错误并点名重复旗标。此前
    /// flag_value 以 find 取首个匹配，重复旗标的后续值被无声丢弃——--state
    /// 这类决定账本落点的旗标静默取错目录尤其危险，必须 fail-loud。
    #[test]
    fn duplicate_flags_are_rejected_as_usage_errors() {
        let args = |parts: &[&str]| -> Vec<String> {
            parts.iter().map(|part| (*part).to_string()).collect()
        };
        let expect_usage = |parts: &[&str], needles: &[&str]| {
            let err = parse_args(&args(parts)).unwrap_err();
            assert!(
                matches!(err, OrderError::Usage { .. }),
                "{parts:?} 应为用法错误: {err}"
            );
            assert_eq!(err.exit_code(), 2, "{parts:?} 用法错误退出码 2: {err}");
            let text = err.to_string();
            assert!(text.starts_with("OR1001:"), "{parts:?} 应报 OR1001: {text}");
            for needle in needles {
                assert!(
                    text.contains(needle),
                    "{parts:?} 应点名重复旗标 {needle}: {text}"
                );
            }
        };

        // --state 重复（决定账本落点，此前静默取首个目录）
        expect_usage(
            &[
                "submit", "--spec", "o.json", "--state", "/a", "--state", "/b",
            ],
            &["--state"],
        );
        // 执行面旗标重复（此前同样静默取首个）
        expect_usage(
            &[
                "submit",
                "--spec",
                "o.json",
                "--state",
                "s",
                "--task",
                "t1",
                "--task",
                "t2",
                "--profile",
                "p",
                "--data-root",
                "d",
            ],
            &["--task"],
        );
        expect_usage(
            &[
                "submit",
                "--spec",
                "o.json",
                "--state",
                "s",
                "--task",
                "t",
                "--profile",
                "p",
                "--data-root",
                "d",
                "--max-turns",
                "5",
                "--max-turns",
                "6",
            ],
            &["--max-turns"],
        );
        // 多个旗标各自重复：一次点名全部
        expect_usage(
            &[
                "submit", "--spec", "a.json", "--spec", "b.json", "--state", "s", "--state", "t",
            ],
            &["--spec", "--state"],
        );
        // list / show 子命令同门
        expect_usage(&["list", "--state", "a", "--state", "b"], &["--state"]);
        expect_usage(
            &["show", "wo-1", "--state", "a", "--state", "b"],
            &["--state"],
        );
    }
}

// ---------- 红队批（W1）辅助：判据面摘要钉死 + 计划图版本读取 ----------

/// W2-C55：任务包计划面 sha256（graph/report 档 = `<data_root>/<task>/plan.json`）
/// ——提交时钉死、凭证面留档（重放对账）、执行前由单元复核。graph/report 两臂
/// 全部预算帽（cost_cap/token_cap/节点 turns）都写在 plan.json 里，而 C-59 的
/// digest 只覆盖 task.json——不钉计划面即可「同 graph_version、同
/// registry_digest 换一份 plan.json」放宽预算且所有装订检查放行。文件缺席
/// = None，不核对。
fn pin_plan_digest(selected: &SelectedUnit, exec: Option<&RealExec>) -> Option<String> {
    let exec = exec?;
    if !matches!(selected, SelectedUnit::Graph | SelectedUnit::Report) {
        return None;
    }
    af_execline::file_sha256_hex(&exec.data_root.join(&exec.task).join("plan.json"))
}

/// W1-C59①：门面 submit 时计算任务包判据面 sha256——swev 档 =
/// `<data_root>/<task>.json`（判分真值源 fail_to_pass/pass_to_pass 所在）；
/// graph/report 档 = `<data_root>/<task>/task.json`（图输入与 check 节点
/// test_command 所在）。执行单元消费前核对（exec::verify_task_digest），
/// 漂移即拒。文件缺席（缺省冒烟/材料后置）= None，不核对。
fn pin_task_digest(selected: &SelectedUnit, exec: Option<&RealExec>) -> Option<String> {
    let exec = exec?;
    let path = match selected {
        SelectedUnit::Swev => exec.data_root.join(format!("{}.json", exec.task)),
        SelectedUnit::Graph | SelectedUnit::Report => {
            exec.data_root.join(&exec.task).join("task.json")
        }
        SelectedUnit::Stub => return None,
    };
    af_execline::file_sha256_hex(&path)
}

/// W1-C32：任务包计划 compiled_from.graph_version（graph/report 档）。
/// 文件缺席/缺键 = None（回落 DEFAULT_GRAPH_VERSION）。
fn read_plan_graph_version(selected: &SelectedUnit, exec: Option<&RealExec>) -> Option<String> {
    let exec = exec?;
    if !matches!(selected, SelectedUnit::Graph | SelectedUnit::Report) {
        return None;
    }
    let plan_path = exec.data_root.join(&exec.task).join("plan.json");
    let plan: Value = serde_json::from_str(&fs::read_to_string(plan_path).ok()?).ok()?;
    plan.pointer("/body/compiled_from/graph_version")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod w1_tests {
    use super::*;

    /// C-59①：pin_task_digest 对 swev/graph/report 各取其判据真值源文件。
    #[test]
    fn pin_task_digest_reads_criteria_source_per_unit() {
        let dir = std::env::temp_dir().join(format!("af-order-w1-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("task-x")).unwrap();
        fs::write(dir.join("task-x.json"), b"swev-task-json").unwrap();
        fs::write(dir.join("task-x/task.json"), b"graph-task-json").unwrap();
        let exec = |task: &str| RealExec {
            data_root: dir.clone(),
            task: task.to_string(),
            host: "local".into(),
            profile: "glm-cp".into(),
            max_turns: Some(60),
            spend_cap_cents: Some(300),
        };
        let swev = pin_task_digest(&SelectedUnit::Swev, Some(&exec("task-x")));
        assert_eq!(
            swev,
            af_execline::file_sha256_hex(&dir.join("task-x.json")),
            "swev 档 = <data_root>/<task>.json"
        );
        let graph = pin_task_digest(&SelectedUnit::Graph, Some(&exec("task-x")));
        assert_eq!(
            graph,
            af_execline::file_sha256_hex(&dir.join("task-x/task.json")),
            "graph 档 = <data_root>/<task>/task.json"
        );
        assert_eq!(
            pin_task_digest(&SelectedUnit::Stub, Some(&exec("task-x"))),
            None,
            "stub 档无判据面"
        );
        assert_eq!(pin_task_digest(&SelectedUnit::Swev, None), None);
        let _ = fs::remove_dir_all(&dir);
    }

    /// C-32：read_plan_graph_version 取任务包计划 compiled_from.graph_version。
    #[test]
    fn read_plan_graph_version_reads_compiled_from() {
        let dir = std::env::temp_dir().join(format!("af-order-w1gv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("task-y")).unwrap();
        fs::write(
            dir.join("task-y/plan.json"),
            r#"{"v":1,"body":{"compiled_from":{"graph_version":"9.9","registry_digest":"0"}}}"#,
        )
        .unwrap();
        let exec = RealExec {
            data_root: dir.clone(),
            task: "task-y".into(),
            host: "local".into(),
            profile: "glm-cp".into(),
            max_turns: Some(60),
            spend_cap_cents: Some(300),
        };
        assert_eq!(
            read_plan_graph_version(&SelectedUnit::Graph, Some(&exec)),
            Some("9.9".to_string())
        );
        assert_eq!(
            read_plan_graph_version(&SelectedUnit::Swev, Some(&exec)),
            None,
            "非计划档不取"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// 红队批（W3-C24）回归：--unit 路由矩阵用法错误的拒绝必须发生在解析期
/// （早于订单目录创建/凭证签发）。主线 438fa6d 曾是「先 create_dir_all →
/// issue_or_load 签凭证 → 才 resolve_unit」——被拒命令在盘上留下
/// voucher.json + 0600 voucher-key.seed，随后同 order_id 改预算的合法
/// submit 被 OR2003「预算不可经重放偷改」永久误拒（合法请求被先前非法
/// 请求毒化）。W1 修复分支序（resolve_unit 先行）合龙后，本测试三段钉死：
/// ①非法 stub+三件套 → OR1001 且 state 根零文件；②改预算合法重放 →
/// 不得 OR2003（引擎定位在凭证签发之后才 fail-loud）；③盘上凭证
/// amount=新预算——重放不被旧值毒化。
#[test]
fn unit_matrix_violation_leaves_zero_residue_and_does_not_poison_budget_replay() {
    let root = std::env::temp_dir().join(format!("af-order-w3c24-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let state = root.join("state");
    let spec = |budget: u64, name: &str| {
        let path = root.join(name);
        fs::write(
            &path,
            format!(
                r#"{{"order_id":"wo-w3c24","line":{{"line_id":"line-A","line_version":"1.0"}},"budget":{budget}}}"#
            ),
        )
        .unwrap();
        path
    };

    // ① 非法形态：--unit stub 与执行三件套同提 = OR1001，state 根零残留
    //    （订单目录、voucher.json、种子一律不得落盘）。
    let spec42 = spec(42, "order-42.json");
    let err = run(&[
        "submit".into(),
        "--spec".into(),
        spec42.display().to_string(),
        "--state".into(),
        state.display().to_string(),
        "--unit".into(),
        "stub".into(),
        "--task".into(),
        "t".into(),
        "--profile".into(),
        "p".into(),
        "--data-root".into(),
        root.display().to_string(),
    ])
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("OR1001") && msg.contains("互斥"), "{msg}");
    fn residue_count(dir: &Path) -> usize {
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| {
                if entry.path().is_dir() {
                    1 + residue_count(&entry.path())
                } else {
                    1
                }
            })
            .sum()
    }
    assert_eq!(
        residue_count(&state),
        0,
        "被拒命令零残留（拒绝发生在解析期，早于目录创建/凭证签发）"
    );

    // ② 改预算合法重放（budget 42→99，合法 stub 形态）：--workspace 指向
    //    空 Cargo 目录 → af-engine 定位/构建 fail-loud（EX1001/EX1002），
    //    该失败点在凭证签发**之后**——能走到它本身就证明 OR2003 毒化门已过。
    let empty_ws = root.join("empty-ws");
    fs::create_dir_all(&empty_ws).unwrap();
    let spec99 = spec(99, "order-99.json");
    let err2 = run(&[
        "submit".into(),
        "--spec".into(),
        spec99.display().to_string(),
        "--state".into(),
        state.display().to_string(),
        "--unit".into(),
        "stub".into(),
        "--workspace".into(),
        empty_ws.display().to_string(),
        "--target-dir".into(),
        root.join("td").display().to_string(),
    ])
    .unwrap_err();
    let msg2 = err2.to_string();
    assert!(
        !msg2.contains("OR2003"),
        "合法改预算重放不得被先前被拒命令的残留误拒（毒化路径）: {msg2}"
    );
    assert!(
        msg2.contains("EX1001") || msg2.contains("EX1002"),
        "引擎面失败发生在凭证签发后（测试缝）: {msg2}"
    );

    // ③ 盘上凭证按新预算签发 = 预算变更被采纳，不被旧值 42 毒化。
    let voucher: Value = serde_json::from_str(
        &fs::read_to_string(state.join("orders").join("wo-w3c24").join("voucher.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        voucher["amount"],
        serde_json::json!(99),
        "凭证 amount 必须是新预算 99（残留毒化则此处为 42 或 OR2003）"
    );
    let _ = fs::remove_dir_all(&root);
}
