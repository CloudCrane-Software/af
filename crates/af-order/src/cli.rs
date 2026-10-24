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

use af_execline::{
    DriverConfig, EngineClient, MesFileWriter, OrderLifecycleDriver, StubExecuteUnit, SystemClock,
};
use af_workorder::{Priority, Sla, StableId};
use serde_json::{json, Value};

use crate::error::OrderError;
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
    Submit {
        spec: PathBuf,
        state: PathBuf,
        workspace: Option<PathBuf>,
        target_dir: Option<PathBuf>,
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
        } => run_submit(&spec, &state, workspace.as_deref(), target_dir.as_deref()),
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
            let (flags, positional) =
                split_flags(rest, &["--spec", "--state", "--workspace", "--target-dir"])?;
            forbid_positional(&positional, "submit")?;
            Ok(Command::Submit {
                spec: PathBuf::from(require_flag(&flags, "--spec")?),
                state: PathBuf::from(require_flag(&flags, "--state")?),
                workspace: flag_value(&flags, "--workspace").map(PathBuf::from),
                target_dir: flag_value(&flags, "--target-dir").map(PathBuf::from),
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

/// 把 `--flag value` 拆进 flags、其余归位置参数。未知 flag / 缺值 = 用法错误。
/// flag 键存全名（含 `--`），与 `require_flag` / `flag_value` 的查询名一致。
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
    Ok((flags, positional))
}

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
) -> Result<String, OrderError> {
    let text = fs::read_to_string(spec_path).map_err(|err| OrderError::SpecRead {
        path: spec_path.display().to_string(),
        detail: err.to_string(),
    })?;
    let spec = parse_spec(&text)?;

    // 状态目录与订单目录（order_id 已过路径安全校验）
    let state = StateDir::new(state_root);
    let order_dir = state.order_dir(&spec.order_id);
    fs::create_dir_all(&order_dir).map_err(|err| OrderError::State {
        detail: format!("创建 {}: {err}", order_dir.display()),
    })?;

    // 签发预算凭证（ed25519；重复 submit 读回原凭证，不重签）
    let stable_id = StableId::generate(&spec.order_id);
    let (voucher, record) = voucher::issue_or_load(&order_dir, &spec, &stable_id)?;

    // 真引擎子进程客户端（定位或构建 af-engine；EX1001/EX1002 fail-loud）
    let workspace = workspace
        .map(PathBuf::from)
        .unwrap_or_else(default_workspace_root);
    let target_dir = target_dir
        .map(PathBuf::from)
        .or_else(default_target_dir)
        .unwrap_or_else(|| workspace.join("target"));
    let client = EngineClient::locate(&workspace, &target_dir)?;

    // 全生命周期驱动（复用 af-execline；同 order_id 重放不重复终态）
    let cfg = DriverConfig {
        order_id: spec.order_id.clone(),
        graph_version: DEFAULT_GRAPH_VERSION.to_string(),
        sla: Sla::new(None, Priority::Normal),
        line: spec.line.clone(),
        voucher,
        out_dir: order_dir.clone(),
        station_id: DEFAULT_STATION_ID.to_string(),
    };
    let mut driver = OrderLifecycleDriver::open(cfg, client, Box::new(SystemClock))?;
    let artifact_id = format!("fa-{}", spec.order_id);
    let report = driver.drive(&StubExecuteUnit::qualified_final(artifact_id.clone()))?;

    let summary = json!({
        "order_id": spec.order_id,
        "stable_id": stable_id.as_str(),
        "state": state::state_name(report.order.state()),
        "replayed": report.replayed,
        "voucher_id": record.voucher_id,
        "voucher_key_id": record.key_id,
        "artifact_id": artifact_id,
        "ledger_ops_appended": report.appended,
        "mes_events_emitted": report.mes_events.len(),
        "order_dir": order_dir.display().to_string(),
        "engine_bin": driver.engine_bin().display().to_string(),
    });
    to_pretty_json(&summary)
}

fn default_workspace_root() -> PathBuf {
    std::env::var_os("AF_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_target_dir() -> Option<PathBuf> {
    std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from)
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
}
