//! af-engine 子进程客户端：9-op 单行 JSON stdin/stdout 协议。
//!
//! 真引擎、不打桩：每次调用拉起一个 af-engine 进程，一行请求进、一行响应出，
//! 然后进程退出（引擎无状态，快照整单进出）。本 crate 不链接 af-engine 库——
//! 库方式调用等于把判定搬进本进程，绕过"必经引擎"门。
//!
//! 二进定位：`<target_dir>/debug/af-engine` 已存在即用；否则在 workspace 根执行
//! `cargo build -p af-engine`（`CARGO_TARGET_DIR` 指向同一 target 目录）。
//! 构建失败或二进制仍缺失 = 明确错误（EX1001 / EX1002），不静默跳过。
//! 构建目录纪律见 ADR-009：只用 `~/afbuild-phase2`，严禁占用 af/target。

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use af_workorder::{DecisionError, Missing, OrderState, Transition};
use serde::Deserialize;
use serde_json::Value;

use crate::error::ExeclineError;

/// af-workorder `GateError` 的线格式镜像（外部标签枚举，变体名与字段名逐字保持）。
///
/// 为什么需要镜像而不是直接用 `af_workorder::GateError`：
/// `GateError::MutationForbidden` 的 `op: &'static str` 字段使
/// `GateError: Deserialize<'de>` 只在 `'de: 'static` 时成立（serde 对 `&str`
/// 字段的隐式借用规则收集 `'static`），无法从运行时缓冲区——引擎 stdout 那行
/// JSON——反序列化。镜像只把 `op` 落成 `String`，线格式一个字不改写。
/// af-workorder 若增删门控变体，本镜像按未知变体/未知字段拒绝（fail-loud，
/// 不静默丢错）。`DecisionError` 无此问题，直接用 af-workorder 原类型。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GateRejection {
    IllegalTransition {
        from: OrderState,
        to: OrderState,
        allowed: Vec<Transition>,
    },
    PreconditionsNotMet {
        transition: Transition,
        missing: Vec<Missing>,
    },
    MutationForbidden {
        state: OrderState,
        op: String,
    },
}

impl GateRejection {
    /// 类型化缺失前置条件清单（仅 `PreconditionsNotMet` 有）。
    /// 与 af-workorder `GateError::missing` 同语义。
    pub fn missing(&self) -> Option<&[Missing]> {
        match self {
            Self::PreconditionsNotMet { missing, .. } => Some(missing),
            _ => None,
        }
    }
}

impl fmt::Display for GateRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IllegalTransition { from, to, allowed } => write!(
                f,
                "非法状态转换 {from:?} → {to:?}（状态机不存在这条边，允许出边 {allowed:?}）"
            ),
            Self::PreconditionsNotMet {
                transition,
                missing,
            } => write!(f, "门控拒绝 {transition:?}: 前置条件缺失 {missing:?}"),
            Self::MutationForbidden { state, op } => write!(f, "状态 {state:?} 下禁止操作 {op}"),
        }
    }
}

/// 引擎协议内拒绝。`kind` 与 af-engine `WireError` 一一对应；
/// `Gate` 用 [`GateRejection`] 镜像承接（见其上文档），`Decision` 是
/// af-workorder 派生 serde 的原样结果（无借用字段，可直接反序列化）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind")]
pub enum EngineRejection {
    BadRequest { message: String },
    RejectionEnvelope { field: String, detail: String },
    Gate { gate: GateRejection },
    Decision { decision: DecisionError },
}

impl fmt::Display for EngineRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest { message } => write!(f, "BadRequest: {message}"),
            Self::RejectionEnvelope { field, detail } => {
                write!(f, "RejectionEnvelope[{field}]: {detail}")
            }
            Self::Gate { gate } => write!(f, "Gate: {gate}"),
            Self::Decision { decision } => write!(f, "Decision: {decision}"),
        }
    }
}

/// 放行响应：整单快照 + 引擎版本（`engine_version` 是编译进二进制的 crate 版本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineAccept {
    pub order: af_workorder::WorkOrder,
    pub engine_version: String,
}

/// 一次引擎调用的结局。放行与拒绝互斥，与协议一致。
///
/// `Accept` 装箱：整单快照大，拒绝支路小；不装箱时 enum 体积被放行支路
/// 撑大（clippy large_enum_variant），而拒绝是高频路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutcome {
    Accept(Box<EngineAccept>),
    Reject(EngineRejection),
}

/// 响应线格式。`ok` 与 `order` / `error` 的互斥由本结构在解析后复核。
#[derive(Debug, Deserialize)]
struct RawResponse {
    ok: bool,
    #[serde(default)]
    order: Option<af_workorder::WorkOrder>,
    #[serde(default)]
    error: Option<EngineRejection>,
    engine_version: String,
}

/// 清空 af-engine 包产物（红队 W2-C56）：`debug/af-engine` 与
/// `debug/deps/af-engine-<hash>` 是**硬链接**（link count=2）——直接写
/// `debug/af-engine` 会穿透污染 deps 缓存，只删 bin 再 build 会被 cargo 从
/// 缓存重新硬链回污染件（0.08s 假 Finished）。必须 clean -p 后再 build，
/// 强制从源码重编译/重链。
fn clean_engine_pkg(workspace_root: &Path, target_dir: &Path) -> Result<(), ExeclineError> {
    let output = Command::new("cargo")
        .args(["clean", "-p", "af-engine"])
        .current_dir(workspace_root)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .map_err(|err| ExeclineError::EngineBuildFailed {
            detail: format!("cargo clean 拉起失败: {err}"),
        })?;
    if !output.status.success() {
        return Err(ExeclineError::EngineBuildFailed {
            detail: format!(
                "cargo clean -p af-engine 退出码 {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(())
}

/// 在 workspace 根构建 af-engine（红队 W2-C56 抽出的共用步）。
fn build_engine(workspace_root: &Path, target_dir: &Path) -> Result<(), ExeclineError> {
    let output = Command::new("cargo")
        .args(["build", "-p", "af-engine"])
        .current_dir(workspace_root)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .map_err(|err| ExeclineError::EngineBuildFailed {
            detail: format!("cargo 拉起失败: {err}"),
        })?;
    if !output.status.success() {
        return Err(ExeclineError::EngineBuildFailed {
            detail: format!(
                "cargo build -p af-engine 退出码 {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(())
}

/// 落信任锚（红队 W2-C56）：`<sha256>  <workspace 根>` 一行。
fn write_trust_anchor(
    anchor: &Path,
    hash: &str,
    workspace_root: &Path,
) -> Result<(), ExeclineError> {
    std::fs::write(anchor, format!("{hash}  {}\n", workspace_root.display())).map_err(|err| {
        ExeclineError::EngineBinaryUntrusted {
            detail: format!("信任锚 {} 写入失败: {err}", anchor.display()),
        }
    })
}

/// 信任锚旁车路径（红队 W2-C56）：与二进制同目录同根，`af-engine.sha256`。
fn trust_anchor_path(bin: &Path) -> PathBuf {
    bin.with_extension("sha256")
}

/// 锚文件内容（trim 后比较；不存在/不可读 = None）。
fn anchor_read(anchor: &Path) -> Option<String> {
    std::fs::read_to_string(anchor)
        .ok()
        .map(|s| s.trim().to_string())
}

/// 文件 sha256 十六进制（不可读 = None）。
fn sha256_of(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(path).ok()?;
    Some(
        Sha256::digest(&data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// af-engine 客户端。
#[derive(Debug, Clone)]
pub struct EngineClient {
    bin: PathBuf,
}

impl EngineClient {
    /// 定位或构建 af-engine 二进制。
    ///
    /// `workspace_root`：af 仓库根（含 `Cargo.toml` workspace）。
    /// `target_dir`：构建产物目录（ADR-009：`~/afbuild-phase2`）。
    ///
    /// **存在且验过即用**（红队 W2-C56）：引擎二进制是全链唯一无 sha256 锚
    /// 的组件，而它正是产出 ledger/MES 里 accept/reject 决策的那一方——此前
    /// 目标目录下的 debug/af-engine 已存在即用（`--target-dir` 还是自由
    /// 路径旗标），被替换的二进制可悄悄改写门控决策。现机制（与判分三件/
    /// render/task.json 的指纹锚同款纪律）：旁车信任锚 `af-engine.sha256`
    /// 记录二进制 sha256 与来源 workspace 根——
    /// - 在场且锚一致 ⇒ 放行；
    /// - 在场无锚 ⇒ 采纳建档（首用登记；此后任何替换即被下一支捕）；
    /// - 在场但锚不符 ⇒ 删旧件从源码重建（构建产物可由源码复现，删之非毁
    ///   数据；重建结果即真身）后重新落锚；
    /// - 不在场 ⇒ 构建并落锚；
    /// - 在场不可读 ⇒ EX1007 拒启动（不静默换路径、不降级）。
    ///
    /// 落锚后终核：锚与二进制必须自洽，否则 EX1007。信任边界与仓内其他指纹
    /// 锚相同：防篡改替换，不防同权限攻击者连锚一起换（需签名根的场景另立）。
    ///
    /// 构建失败 = EX1002，构建后仍找不到二进制 = EX1001——都不静默跳过。
    pub fn locate(workspace_root: &Path, target_dir: &Path) -> Result<Self, ExeclineError> {
        let bin = target_dir.join("debug").join("af-engine");
        let anchor = trust_anchor_path(&bin);
        if bin.is_file() {
            match (sha256_of(&bin), anchor_read(&anchor)) {
                // ① 验过即用。
                (Some(hash), Some(record)) if record.starts_with(&hash) => {}
                // ② 首用采纳建档。
                (Some(hash), None) => write_trust_anchor(&anchor, &hash, workspace_root)?,
                // ③ 锚不符（被替换/源码已重建）：回到可复现的源码构建。
                (Some(_), Some(_)) => {
                    let _ = std::fs::remove_file(&anchor);
                    clean_engine_pkg(workspace_root, target_dir)?;
                    build_engine(workspace_root, target_dir)?;
                    let hash =
                        sha256_of(&bin).ok_or_else(|| ExeclineError::EngineBinaryUntrusted {
                            detail: format!("重建后 {} 不可读（无法验真）", bin.display()),
                        })?;
                    write_trust_anchor(&anchor, &hash, workspace_root)?;
                }
                // ④ 在场不可读：拒启动。
                (None, _) => {
                    return Err(ExeclineError::EngineBinaryUntrusted {
                        detail: format!("{} 不可读（无法验真，拒启动）", bin.display()),
                    });
                }
            }
        } else {
            build_engine(workspace_root, target_dir)?;
            let hash = sha256_of(&bin).ok_or_else(|| ExeclineError::EngineBinaryUntrusted {
                detail: format!("构建后 {} 不可读（无法验真）", bin.display()),
            })?;
            write_trust_anchor(&anchor, &hash, workspace_root)?;
        }
        if !bin.is_file() {
            return Err(ExeclineError::EngineBinaryMissing {
                path: bin.display().to_string(),
            });
        }
        // 终核：锚必须在场且与二进制一致（写盘竞态/文件系统异常 = 不启动）。
        let hash = sha256_of(&bin).ok_or_else(|| ExeclineError::EngineBinaryUntrusted {
            detail: format!("{} 不可读（无法验真）", bin.display()),
        })?;
        if !anchor_read(&anchor)
            .as_deref()
            .is_some_and(|a| a.starts_with(&hash))
        {
            return Err(ExeclineError::EngineBinaryUntrusted {
                detail: format!(
                    "信任锚终核不过：{} 与 {} 不一致（拒启动）",
                    anchor.display(),
                    bin.display()
                ),
            });
        }
        Ok(Self { bin })
    }

    /// 二进制路径。证明判定走的是真引擎进程，而非进程内库调用。
    pub fn bin(&self) -> &Path {
        &self.bin
    }

    /// 一次 op 调用。`request` 是完整的一行请求对象（含 `op` / `order` / 该 op 参数）。
    ///
    /// 进程协议：一行请求写 stdin，读一行 stdout，进程退出。协议内的放行和
    /// 拒绝都退出 0；非零退出 / 空 stdout / 坏 JSON 才是传输层错误
    /// （EX1003 / EX1004 / EX1005）。
    pub fn call(&self, request: &Value) -> Result<EngineOutcome, ExeclineError> {
        let line =
            serde_json::to_string(request).map_err(|err| ExeclineError::EngineBadResponse {
                detail: format!("请求序列化失败: {err}"),
            })?;
        let mut child = Command::new(&self.bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| ExeclineError::EngineSpawnFailed {
                detail: format!("{}: {err}", self.bin.display()),
            })?;
        {
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| ExeclineError::EngineSpawnFailed {
                    detail: "stdin 管道不可用".into(),
                })?;
            stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.write_all(b"\n"))
                .map_err(|err| ExeclineError::EngineSpawnFailed {
                    detail: format!("stdin 写入失败: {err}"),
                })?;
        }
        let output = child
            .wait_with_output()
            .map_err(|err| ExeclineError::EngineCrashed {
                detail: format!("等待 af-engine 退出失败: {err}"),
            })?;
        if !output.status.success() {
            return Err(ExeclineError::EngineCrashed {
                detail: format!(
                    "退出码 {:?}: {}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        let stdout =
            String::from_utf8(output.stdout).map_err(|err| ExeclineError::EngineBadResponse {
                detail: format!("stdout 不是 UTF-8: {err}"),
            })?;
        let response_line =
            stdout
                .lines()
                .next()
                .ok_or_else(|| ExeclineError::EngineBadResponse {
                    detail: "stdout 为空：af-engine 未写响应行".into(),
                })?;
        let raw: RawResponse = serde_json::from_str(response_line).map_err(|err| {
            ExeclineError::EngineBadResponse {
                detail: format!("响应行不是合法 JSON: {err}（原文: {response_line}）"),
            }
        })?;
        match (raw.ok, raw.order, raw.error) {
            (true, Some(order), None) => Ok(EngineOutcome::Accept(Box::new(EngineAccept {
                order,
                engine_version: raw.engine_version,
            }))),
            (false, None, Some(error)) => Ok(EngineOutcome::Reject(error)),
            (true, None, _) => Err(ExeclineError::EngineBadResponse {
                detail: "ok=true 但响应缺 order 快照".into(),
            }),
            (false, _, None) => Err(ExeclineError::EngineBadResponse {
                detail: "ok=false 但响应缺 error".into(),
            }),
            (true, Some(_), Some(_)) => Err(ExeclineError::EngineBadResponse {
                detail: "order 与 error 同时出现，违反协议互斥".into(),
            }),
            (false, order, error) => Err(ExeclineError::EngineBadResponse {
                detail: format!(
                    "ok=false 但响应不合法: order={} error={}",
                    order.is_some(),
                    error.is_some()
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 拒绝线按 `kind` 内标签解析，Gate 的缺失清单保持类型化。
    #[test]
    fn rejection_wire_kinds_parse_with_typed_missing() {
        let gate: EngineRejection = serde_json::from_str(
            r#"{"kind":"Gate","gate":{"PreconditionsNotMet":{"transition":"Release","missing":["BudgetVoucherNotIssued"]}}}"#,
        )
        .expect("Gate 拒绝线应可解析");
        match &gate {
            EngineRejection::Gate { gate } => {
                assert_eq!(
                    gate.missing().unwrap(),
                    &[af_workorder::Missing::BudgetVoucherNotIssued],
                    "缺失清单类型化，顺序保持库的固定顺序"
                );
            }
            other => panic!("期望 Gate，实际 {other:?}"),
        }
        assert!(gate.to_string().contains("Gate:"), "{gate}");
        assert!(
            gate.to_string().contains("BudgetVoucherNotIssued"),
            "{gate}"
        );

        let mutation: EngineRejection = serde_json::from_str(
            r#"{"kind":"Gate","gate":{"MutationForbidden":{"state":"Released","op":"bind_line"}}}"#,
        )
        .expect("MutationForbidden 拒绝线应可解析");
        match &mutation {
            EngineRejection::Gate { gate } => {
                assert!(matches!(gate, GateRejection::MutationForbidden { .. }));
                assert!(gate.missing().is_none(), "MutationForbidden 没有缺失清单");
                assert!(mutation.to_string().contains("bind_line"), "{mutation}");
            }
            other => panic!("期望 Gate，实际 {other:?}"),
        }

        let bad: EngineRejection =
            serde_json::from_str(r#"{"kind":"BadRequest","message":"缺少字段 op"}"#)
                .expect("BadRequest 拒绝线应可解析");
        assert!(matches!(bad, EngineRejection::BadRequest { .. }));
    }

    /// 线格式以外的变体名按未知拒绝，不静默当成别的东西。
    #[test]
    fn unknown_gate_variant_is_rejected_not_ignored() {
        let err = serde_json::from_str::<EngineRejection>(
            r#"{"kind":"Gate","gate":{"SomeFutureVariant":{"x":1}}}"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unknown variant"),
            "未知门控变体必须拒绝: {err}"
        );
    }
}

// ---------- 红队修复批（W2）：引擎二进制信任锚 ----------

#[test]
fn engine_binary_tamper_is_detected_and_rebuilt() {
    // 红队 W2-C56：锚建档后替换二进制 → locate 必须发现（锚 ≠ 实货）并从
    // 源码重建恢复真身——此前「已存在即用」，被替换的引擎可悄悄改写门控
    // 决策（它正是产出 accept/reject 的一方）。用独立 target 目录复刻
    // （不影响在役构建目录）。
    use std::path::PathBuf;
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let target = std::env::temp_dir().join(format!("af-engine-anchor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&target);

    let client = EngineClient::locate(&workspace, &target).expect("首次 locate 应构建并落锚");
    let bin = client.bin().to_path_buf();
    let anchor = trust_anchor_path(&bin);
    let genuine = std::fs::read(&bin).expect("真身可读");
    let anchored = anchor_read(&anchor).expect("锚应在场");
    assert!(
        anchored.starts_with(&sha256_of(&bin).unwrap()),
        "锚内容 = 实货 sha256 + 来源"
    );

    // 篡改：附加一字节（哈希必变）。
    let mut tampered = genuine.clone();
    tampered.push(b'x');
    std::fs::write(&bin, &tampered).expect("写入篡改件");
    let tampered_hash = sha256_of(&bin).unwrap();
    assert!(
        !anchored.starts_with(&tampered_hash),
        "篡改后哈希应与锚不同（测试前提）"
    );

    // 二次 locate：捕篡改 → 删旧重建 → 真身恢复（构建非逐位可复现——debug
    // 产物含增量状态，故不断言字节相等，断言安全性质：篡改件不得在用）。
    let client2 = EngineClient::locate(&workspace, &target).expect("二次 locate 应重建恢复");
    let restored_hash = sha256_of(client2.bin()).expect("重建后可读");
    assert_ne!(
        restored_hash, tampered_hash,
        "篡改件不得留在执行位（重建后哈希必须回到真身域）"
    );
    assert!(
        anchor_read(&anchor)
            .expect("重建后锚应在场")
            .starts_with(&restored_hash),
        "重建后锚与在用二进制自洽（终核性质）"
    );
    // 三次 locate：验过即用（不再触发重建——路径上无删除动作）。
    let client3 = EngineClient::locate(&workspace, &target).expect("三次 locate 应验过即用");
    assert_eq!(
        sha256_of(client3.bin()).expect("可读"),
        restored_hash,
        "验过即用：二进制不被反复重建"
    );

    let _ = std::fs::remove_dir_all(&target);
}
