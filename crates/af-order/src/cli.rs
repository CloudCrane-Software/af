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
        /// E1 缺口①：干跑档——提交前链全走（路由/摘要钉死/任务包材料面/计划
        /// 装订预检），零副作用（不落状态目录、不签凭证、不起引擎），stdout
        /// 留证即返回。std 解析器只收 `--flag value` 形，故取 true|false 显式值。
        dry_run: bool,
    },
    /// 列出全部订单及终态。
    List { state: PathBuf },
    /// 打印单订单 ledger 摘要与 MES 事件。
    Show { order_id: String, state: PathBuf },
    /// 裁决受理（PoC-A2）：DecisionArtifact 按固定校验链接收入单，
    /// 全过后才写 `admissions.jsonl`；receipt 带幂等键（同键同摘要重放
    /// 返回原 receipt，同 seq 异摘要拒绝）。`now` = 验签时刻覆写（测试/复现）。
    Admit {
        request: PathBuf,
        state: PathBuf,
        workspace: Option<PathBuf>,
        target_dir: Option<PathBuf>,
        now: Option<String>,
    },
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
            dry_run,
        } => run_submit(
            &spec,
            &state,
            workspace.as_deref(),
            target_dir.as_deref(),
            exec.as_ref(),
            &SubmitOpts {
                unit,
                mes_ch,
                dry_run,
            },
        ),
        Command::List { state } => run_list(&state),
        Command::Show { order_id, state } => run_show(&order_id, &state),
        Command::Admit {
            request,
            state,
            workspace,
            target_dir,
            now,
        } => run_admit(
            &request,
            &state,
            workspace.as_deref(),
            target_dir.as_deref(),
            now.as_deref(),
        ),
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
            allowed.push("--dry-run"); // E1 缺口①：干跑档（true|false）
            let (flags, positional) = split_flags(rest, &allowed)?;
            forbid_positional(&positional, "submit")?;
            // 执行面三件套规则与缺省见 crate::exec（B① 正式契约）；
            // --mes-ch 是记账声明（与执行面正交），原样透传驱动缝；
            // --dry-run 是布尔档（std 解析器只收 `--flag value` 形，显式 true|false）。
            let exec = parse_exec_flags(&flags)?;
            let dry_run = match flag_value(&flags, "--dry-run") {
                None => false,
                Some("true") => true,
                Some("false") => false,
                Some(other) => {
                    return Err(OrderError::Usage {
                        detail: format!("--dry-run 只接受 true|false（收到 {other:?}）"),
                    })
                }
            };
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
                dry_run,
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
        "admit" => {
            let (flags, positional) = split_flags(
                rest,
                &[
                    "--request",
                    "--state",
                    "--workspace",
                    "--target-dir",
                    "--now",
                ],
            )?;
            forbid_positional(&positional, "admit")?;
            let now = flag_value(&flags, "--now").map(str::to_string);
            if let Some(raw) = &now {
                // 解析期即拒坏时间（OR1001），不等到链内才失败。
                chrono::DateTime::parse_from_rfc3339(raw).map_err(|err| OrderError::Usage {
                    detail: format!("--now 不是合法 RFC3339: {err}"),
                })?;
            }
            Ok(Command::Admit {
                request: PathBuf::from(require_flag(&flags, "--request")?),
                state: PathBuf::from(require_flag(&flags, "--state")?),
                workspace: flag_value(&flags, "--workspace").map(PathBuf::from),
                target_dir: flag_value(&flags, "--target-dir").map(PathBuf::from),
                now,
            })
        }
        other => Err(OrderError::Usage {
            detail: format!("未知子命令 {other}（可选 submit/admit/list/show）"),
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

/// submit 的路由/记账/干跑档面（参数面三聚一，避 clippy::too_many_arguments；
/// 字段语义各自独立不变）。
struct SubmitOpts {
    /// 单元路由（裁决7：swev|report|stub；Auto=缺省现行为）。
    unit: UnitChoice,
    /// MES CH 目标（"auto" 或显式 URL；None = 只落文件正本）。
    mes_ch: Option<String>,
    /// E1 缺口①：干跑档——提交前链全走、零副作用留证（true|false 显式值）。
    dry_run: bool,
}

fn run_submit(
    spec_path: &Path,
    state_root: &Path,
    workspace: Option<&Path>,
    target_dir: Option<&Path>,
    exec: Option<&RealExec>,
    opts: &SubmitOpts,
) -> Result<String, OrderError> {
    let text = fs::read_to_string(spec_path).map_err(|err| OrderError::SpecRead {
        path: spec_path.display().to_string(),
        detail: err.to_string(),
    })?;
    let mut spec = parse_spec(&text)?;

    // 红队批（W1）三件先行计算（均在凭证签发前——凭证/订单/MES 三面同值）：
    // ① 单元路由（stub 判别）；② 任务包判据面 sha256（W1-C59）；③ 计划
    //    graph_version（W1-C32：门面按计划图版本取值，统一值域）。
    let selected = resolve_unit(opts.unit, exec)?;
    let task_digest = pin_task_digest(&selected, exec);
    let plan_digest = pin_plan_digest(&selected, exec);
    let plan_graph_version = read_plan_graph_version(&selected, exec)
        .unwrap_or_else(|| DEFAULT_GRAPH_VERSION.to_string());
    // E1 缺口② + G3：任务包内容 hash 一致性（W2-C55 防重放语义内置提交链）
    // ——任务包 manifest（编译链组包时留档 task.json/plan.json 内容 sha256）
    // 在场即重算核对，不符拒于一切副作用之前（清态换号重提指令在
    // af-compiler verify 正本文案面）；G3 起 graph/report 档 manifest 缺席
    // 不再「= 不核对」放行，preflight fail-closed 拒绝（见其注释）。
    let package_manifest = preflight_package_manifest(&selected, exec)?;
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

    // E1 缺口①：干跑档——提交前链（至此已完成 spec 解析/单元路由/摘要钉死/
    // 图版本读取/任务包 manifest 一致性）在此留证返回，零副作用：不落状态
    // 目录、不签凭证、不起引擎。
    if opts.dry_run {
        return run_submit_dryrun(
            &spec,
            &selected,
            exec,
            &task_digest,
            &plan_digest,
            &plan_graph_version,
            &package_manifest,
        );
    }

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
        attempt_layout: match opts.unit {
            // 图执行器：graph- 前缀 D-20 递增布局（套件面区分）。
            UnitChoice::Graph => AttemptLayout::GraphIncrement,
            _ => AttemptLayout::SwevIncrement,
        },
    };
    // MES 写缝（B②③）：--mes-ch 只是目标声明（与执行面正交），写侧
    // （含读侧对账）全在 af-execline 的 MesChSink；不声明 = 文件正本。
    let mes_ch_target = opts.mes_ch.as_deref().map(MesChTarget::parse).transpose()?;
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

/// E1 缺口①：submit 干跑档——提交前链全走、零副作用留证。
///
/// 走完的真实链（与正式 submit 同源同序，绝不复刻简化）：
/// ① spec 解析 + 单元路由 + 摘要钉死（task/plan sha256）+ 计划图版本读取
///    ——run_submit 前段已执行，本函数直接消费其结果；
/// ② graph 臂：任务包材料面装配（`graph_unit_for`：task.json/plan.json 布
///    局 + registry_root 解析 + 生产判分锚接线）；
/// ③ 任务包 manifest 一致性（E1 缺口②：编译链留档的内容 sha256 重算核对，
///    不符即拒——清态换号重提；G3：缺席亦拒，fail-closed）+ 计划装订预检
///    （`planbind_bind` 全四核对：
///    信封 decode/E9004 注册表 digest/一致性/模板清单）；
/// ④ 基座档案可解析（装载期同款门）。
///
/// 零副作用承诺：不建状态目录、不签凭证、不定位/构建引擎、不驱动订单——
/// 干跑失败同样零残留（W3-C24 纪律对干跑档同样成立）。
fn run_submit_dryrun(
    spec: &crate::spec::OrderSpec,
    selected: &SelectedUnit,
    exec: Option<&RealExec>,
    task_digest: &Option<String>,
    plan_digest: &Option<String>,
    plan_graph_version: &str,
    package_manifest: &Option<af_compiler::TaskPackageManifest>,
) -> Result<String, OrderError> {
    let mut checks: Vec<&str> = vec![
        "parse_spec",
        "resolve_unit",
        "pin_task_digest",
        "pin_plan_digest",
        "read_plan_graph_version",
    ];
    // G3：留证面不许把「不适用」写成含混的 null——核对过 = manifest 留档
    // 对象；不适用（非 graph/report 档或无 RealExec）= 显式 "not_applicable"。
    // 缺席不会走到这里：要求核对的路径上缺席已在 preflight 拒绝。
    let package_manifest_evidence = match package_manifest {
        Some(manifest) => {
            checks.push("package_manifest");
            serde_json::to_value(manifest).unwrap_or_else(|_| serde_json::json!("serialize_failed"))
        }
        None => serde_json::json!("not_applicable"),
    };
    let unit_detail = match selected {
        SelectedUnit::Graph => {
            let exec = exec.expect("resolve 已保证 Graph 携三件套");
            // ② 任务包材料面（与正式 submit 同一装配函数）。
            let unit_spec = graph_unit_for(exec)?;
            checks.push("graph_task_package_layout");
            // ③ 计划装订预检：与图执行器 setup 相①同一份 planbind 正本。
            let plan_path = exec.data_root.join(&exec.task).join("plan.json");
            let plan_src = fs::read_to_string(&plan_path).map_err(|err| OrderError::Usage {
                detail: format!("OR1001: 干跑读计划失败（{}）: {err}", plan_path.display()),
            })?;
            let bound = af_execline::planbind_bind(
                &plan_src,
                &unit_spec.registry_root,
                &[],
                &af_execline::BindExpectations {
                    graph_version: Some(plan_graph_version.to_string()),
                    // 与驱动层 bind_line 后送达执行面的期待同源（计划编译期
                    // line 常 None，此期待仅在计划携带 line_id 时触发核对）。
                    line_id: Some(spec.line.line_id.clone()),
                },
            )
            .map_err(|err| OrderError::Usage {
                detail: format!("OR1001: 干跑计划装订预检未过: {err}"),
            })?;
            checks.push("planbind_preflight");
            // ④ 基座档案可解析（GraphExecuteUnit::new 装载期同款门）。
            af_execline::profile::profile(&unit_spec.profile).map_err(|err| OrderError::Usage {
                detail: format!("OR1001: 干跑档案预检未过: {err}"),
            })?;
            checks.push("profile_resolvable");
            json!({
                "unit": "graph-unit",
                "plan_id": bound.plan.body.plan_id,
                "input_type": bound.plan.body.interface.input_type,
                "output_type": bound.plan.body.interface.output_type,
                "registry_root": unit_spec.registry_root.display().to_string(),
                "judge_wired": unit_spec.judge.is_some(),
            })
        }
        other => {
            // 非 graph 臂：本批（E1 建设任务链）只把 graph 臂的材料面+装订
            // 预检接线到干跑档；其余臂如实标注未接线，不复刻简化预检冒充全链。
            json!({
                "unit": match other {
                    SelectedUnit::Swev => "swev-unit",
                    SelectedUnit::Report => "report-unit",
                    SelectedUnit::Stub => "stub-unit",
                    SelectedUnit::Graph => unreachable!("graph 臂已收"),
                },
                "materials_preflight": "not_wired_in_dryrun_v1",
            })
        }
    };
    let out = json!({
        "dry_run": true,
        "order_id": spec.order_id,
        "budget": spec.budget,
        "unit_detail": unit_detail,
        "task_sha256": task_digest,
        "plan_sha256": plan_digest,
        "graph_version": plan_graph_version,
        "package_manifest": package_manifest_evidence,
        "checks": checks,
        "note": "干跑档：零副作用（无状态目录/凭证/引擎），编译→提交一跳预检留证，不真下单",
    });
    to_pretty_json(&out)
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

// ---------- admit ----------

/// 裁决受理：读 admission.json → 解析（OR2007）→ 定位引擎 → 走 admit 校验链。
/// receipt 落 `admissions.jsonl` 后随引擎路径一并 stdout 留证。
fn run_admit(
    request_path: &Path,
    state_root: &Path,
    workspace: Option<&Path>,
    target_dir: Option<&Path>,
    now: Option<&str>,
) -> Result<String, OrderError> {
    let text = fs::read_to_string(request_path).map_err(|err| OrderError::RequestRead {
        path: request_path.display().to_string(),
        detail: err.to_string(),
    })?;
    let request = crate::admit::parse_request(&text).map_err(|err| OrderError::RequestInvalid {
        detail: err.to_string(),
    })?;
    // 验签/受理时刻：--now 显式覆写（复现/测试），否则请求体内 validated_at，
    // 再否则系统钟。三处同源进链，receipt.admitted_at 如实记录实际所用值。
    let validated_at = match now {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(raw)
            .map_err(|err| OrderError::Usage {
                detail: format!("--now 不是合法 RFC3339: {err}"),
            })?
            .with_timezone(&chrono::Utc),
        None => request.validated_at.unwrap_or_else(chrono::Utc::now),
    };

    let (workspace, target_dir) = resolve_build_dirs(workspace, target_dir);
    let client = EngineClient::locate(&workspace, &target_dir)?;
    let state = StateDir::new(state_root);
    let receipt = crate::admit::admit(&request, &state, &client, validated_at)?;

    let mut out = serde_json::to_value(&receipt).map_err(|err| OrderError::State {
        detail: format!("回执序列化失败: {err}"),
    })?;
    out.as_object_mut().expect("回执是对象").insert(
        "engine_bin".into(),
        Value::String(client.bin().display().to_string()),
    );
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
                // E1 缺口①：干跑档缺省 false（不给旗标 = 正式提交，行为零漂移）。
                dry_run: false,
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

        // admit 形状：必填 --request/--state，可选 --workspace/--target-dir/--now。
        let Command::Admit {
            request,
            state,
            workspace,
            target_dir,
            now,
        } = parse_args(&args(&[
            "admit",
            "--request",
            "a.json",
            "--state",
            "st",
            "--now",
            "2026-11-03T00:00:00Z",
        ]))
        .unwrap()
        else {
            panic!("应解析为 Admit");
        };
        assert_eq!(request, PathBuf::from("a.json"));
        assert_eq!(state, PathBuf::from("st"));
        assert_eq!(workspace, None);
        assert_eq!(target_dir, None);
        assert_eq!(now.as_deref(), Some("2026-11-03T00:00:00Z"));

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

/// E1 缺口② + G3 收口：提交前任务包 manifest 一致性核对（W2-C55 防重放
/// 语义内置提交链的编译链侧补齐）。任务包 `package_manifest.json`（编译链
/// 组包时由 af-compiler 写入，留档 task.json/plan.json 内容 sha256）在场即
/// 重算核对——任一文件与留档不符 = 内容被改（E2G2 实证：预算帽全在
/// plan.json，换计划面 = 放宽预算），拒于一切副作用之前，文案点名「清态
/// 换号重提」。G3：graph/report 档（携带 RealExec）是**要求核对**的路径——
/// manifest 缺席不再「= 不核对」静默放行（旧口径下删 manifest 即绕过核对，
/// 实证可整单真跑），而是 fail-closed 拒绝；缺席与核对过在 af-compiler
/// `ManifestVerification` 类型上分开。非 graph/report 档或无 RealExec =
/// 核对不适用，返回 None（干跑留证面以 "not_applicable" 显式标注，不与
/// 「核对过」混同）。正式 submit 与 --dry-run 两路同点调用（本函数在干跑
/// 分支之前）。
fn preflight_package_manifest(
    selected: &SelectedUnit,
    exec: Option<&RealExec>,
) -> Result<Option<af_compiler::TaskPackageManifest>, OrderError> {
    let Some(exec) = exec else {
        return Ok(None);
    };
    if !matches!(selected, SelectedUnit::Graph | SelectedUnit::Report) {
        return Ok(None);
    }
    let task_dir = exec.data_root.join(&exec.task);
    let manifest_path = task_dir.join(af_compiler::PACKAGE_MANIFEST_FILE);
    match af_compiler::verify_package_manifest(&task_dir) {
        Ok(af_compiler::ManifestVerification::Verified(manifest)) => Ok(Some(manifest)),
        Ok(af_compiler::ManifestVerification::Absent) => Err(OrderError::Usage {
            detail: format!(
                "OR1001: 任务包缺 {}（{}）：G3 fail-closed——要求核对的路径上 manifest 缺席 = \
                 拒，不放行（未组包或 manifest 被移除均不可静默当「核对过」；删 manifest 曾可\
                 绕过内容核对）；请经 af-compiler 组包（assemble_construction_package：组包即\
                 留档 task/plan 内容 sha256）重产任务包，勿手工拼包后直接提交",
                af_compiler::PACKAGE_MANIFEST_FILE,
                manifest_path.display(),
            ),
        }),
        Err(err) => Err(OrderError::Usage {
            detail: format!("OR1001: {err}"),
        }),
    }
}

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
