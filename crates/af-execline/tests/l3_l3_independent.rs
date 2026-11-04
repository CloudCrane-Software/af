//! L3'/L3'' 独立补丁回合的新增测试（零容器 / 零 ssh / 零网络 / 零真实判分器，可复跑）。
//!
//! 覆盖（任务书 §四.5）：
//! - L3' 缺省 180s；
//! - L3' 超上限被夹到 600s；
//! - L3'' 终态结果不重试；
//! - L3'' 瞬态错误重试并退避（退避 5s / 15s）。
//!
//! 另含两项**与 fix3 的逐条语义等价性对拍**（对照实现逐字抄自 fix3 源
//! `b3e1f56`：`crates/af-bench/src/main.rs::bash_timeout_sec` 与
//! `crates/af-bench/src/judge.rs::run_judge` 的循环骨架）——同输入下本实现与
//! fix3 的行为差异为空。
//!
//! 另含**缺省行为逐字不变**（C 门）断言：未配置新参数时，bash 桥接超时仍 180s、
//! 超时文案与基线逐字节一致；judge 仍单次调用（不等待、不重试、注记不带
//! `JUDGE_ERROR:` 前缀）。

use std::sync::Mutex;

use af_execline::hostbridge::HostBridge;
use af_execline::suite::swev::{
    judge_call_with_retry, judge_retry_enabled, judge_retry_enabled_from, JudgeAttempt,
    JudgeCallOutcome, JUDGE_BACKOFF_SEC, JUDGE_MAX_ATTEMPTS, JUDGE_RETRY_ENV,
};
use af_execline::suite::tb::{
    bash_timeout_sec, bash_timeout_sec_from_env, TbEnv, BASH_TIMEOUT_ENV, DEFAULT_BASH_TIMEOUT_SEC,
    MAX_BASH_TIMEOUT_SEC,
};
use serde_json::{json, Value};

// ---- 环境变量串行闸（测试进程内改 env 需串行；本文件内所有读 env 的用例共用）----
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// 记录桥接调用的假桥接（零容器）。
#[derive(Default)]
struct RecordingBridge {
    calls: Mutex<Vec<(String, String, u32)>>,
    reply: Mutex<Vec<Result<(bool, String), String>>>,
}

impl RecordingBridge {
    fn with_replies(replies: Vec<Result<(bool, String), String>>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            reply: Mutex::new(replies),
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.lock().unwrap().clone()
    }
}

impl HostBridge for RecordingBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String> {
        self.calls
            .lock()
            .unwrap()
            .push((host.to_string(), script.to_string(), timeout_sec));
        let mut reply = self.reply.lock().unwrap();
        if reply.is_empty() {
            return Ok((true, String::new()));
        }
        reply.remove(0)
    }
}

fn env_of() -> TbEnv {
    TbEnv {
        container: "afbench-hello-world-glm-cp".into(),
        task: "hello-world".into(),
        host: "anolis".into(),
        workdir: "/app".into(),
    }
}

/// 基线（origin/product/main `crates/af-execline/src/suite/tb.rs`）的超时文案输出——
/// 逐字复制，用于"缺省行为逐字不变"的对照。
const BASELINE_TIMEOUT_OUTPUT: &str = "partial output\n\n[harness] The command was killed after exceeding the 180s timeout (no exit code was reported). Partial output (if any) is above. Re-run with a different approach: pipe input or use non-interactive flags (-y / -n / DEBIAN_FRONTEND=noninteractive), wrap long work in an explicit `timeout <secs> ...`, and avoid commands that never exit on their own (servers, watch loops, pagers).\nexit=124";

// ============================ L3'：bash 超时可调 ============================

/// L3'-1：缺省 180s（历史口径逐字不变）。
#[test]
fn l3prime_default_is_180_seconds() {
    assert_eq!(DEFAULT_BASH_TIMEOUT_SEC, 180);
    // 缺失 / 0 / 小于 1s / 恰好 180000ms → 缺省 180s。
    assert_eq!(bash_timeout_sec(None), 180);
    assert_eq!(bash_timeout_sec(Some(0)), 180);
    assert_eq!(bash_timeout_sec(Some(1)), 180);
    assert_eq!(bash_timeout_sec(Some(999)), 180);
    assert_eq!(bash_timeout_sec(Some(180_000)), 180);
}

/// L3'-2：超上限被夹到 600s（超时闸是护栏，不许无上限）。
#[test]
fn l3prime_over_cap_is_clamped_to_600_seconds() {
    assert_eq!(MAX_BASH_TIMEOUT_SEC, 600);
    assert_eq!(bash_timeout_sec(Some(600_000)), 600);
    assert_eq!(bash_timeout_sec(Some(600_001)), 600);
    assert_eq!(bash_timeout_sec(Some(900_000)), 600);
    assert_eq!(bash_timeout_sec(Some(u64::MAX)), 600);
    // 下界：1000ms → 1s（不落 0，避免"零超时"这一未定义形态）。
    assert_eq!(bash_timeout_sec(Some(1_000)), 1);
    assert_eq!(bash_timeout_sec(Some(1_999)), 1);
}

/// L3'-3：与 fix3 的换算函数逐条等价（同输入同输出）。
///
/// 对照实现逐字抄自 fix3 `b3e1f56` `crates/af-bench/src/main.rs::bash_timeout_sec`
/// （常量内联为 fix3 的 180/600，函数体一字不改）。
fn fix3_bash_timeout_sec(call: &Value) -> u32 {
    const DEFAULT_BASH_TIMEOUT_SEC: u32 = 180;
    const MAX_BASH_TIMEOUT_SEC: u32 = 600;
    let requested_ms = call.get("timeout_ms").and_then(Value::as_u64);
    match requested_ms {
        Some(ms) if ms >= 1_000 => {
            let sec = (ms / 1_000) as u32;
            sec.clamp(1, MAX_BASH_TIMEOUT_SEC)
        }
        _ => DEFAULT_BASH_TIMEOUT_SEC,
    }
}

#[test]
fn l3prime_matches_fix3_conversion_clause_by_clause() {
    let cases = vec![
        json!({}),                        // 缺参
        json!({"timeout_ms": null}),      // null
        json!({"timeout_ms": 0}),         // 0
        json!({"timeout_ms": 1}),         // 1
        json!({"timeout_ms": 999}),       // < 1s
        json!({"timeout_ms": 1000}),      // 1s
        json!({"timeout_ms": 1999}),      // 整除到 1s
        json!({"timeout_ms": 180_000}),   // 缺省值
        json!({"timeout_ms": 180_001}),   // 整除到 180s
        json!({"timeout_ms": 300_000}),   // 300s
        json!({"timeout_ms": 599_999}),   // 整除到 599s
        json!({"timeout_ms": 600_000}),   // 上限
        json!({"timeout_ms": 600_001}),   // 超上限 → 夹到 600
        json!({"timeout_ms": 3_600_000}), // 1h → 夹到 600
        json!({"timeout_ms": -1}),        // 负数（非 u64）
        json!({"timeout_ms": 1.5}),       // 非整数
        json!({"timeout_ms": "180000"}),  // 字符串
        json!({"timeout_ms": true}),      // 布尔
    ];
    for call in cases {
        let fix3 = fix3_bash_timeout_sec(&call);
        let mine = bash_timeout_sec(call.get("timeout_ms").and_then(Value::as_u64));
        assert_eq!(mine, fix3, "换算不等价: {call}");
    }
    // 夹取域外值（fix3 的 exec 臂另有 clamp(1, 600)）同样一致。
    for sec in [0u32, 1, 180, 599, 600, 601, 1000] {
        assert_eq!(
            sec.clamp(1, MAX_BASH_TIMEOUT_SEC),
            sec.clamp(1, 600),
            "夹取不等价: {sec}"
        );
    }
}

/// L3'-4：缺省未配置时，桥接超时仍 180s 且超时文案与基线**逐字节一致**（C 门）。
#[test]
fn l3prime_default_exec_output_is_byte_identical_to_baseline() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var(BASH_TIMEOUT_ENV);
    assert_eq!(bash_timeout_sec_from_env(), 180);

    let bridge = RecordingBridge::with_replies(vec![Ok((true, "partial output\n".into()))]);
    let out = env_of().exec(&bridge, "sleep 999");
    assert_eq!(out, BASELINE_TIMEOUT_OUTPUT, "缺省输出与基线不逐字节一致");
    let calls = bridge.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].2, 180, "缺省桥接超时不是 180s");
}

/// L3'-5：配置生效时，桥接超时与文案里的秒数**跟着实际值走**（不留硬编码 180）。
#[test]
fn l3prime_configured_timeout_flows_into_bridge_and_message() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var(BASH_TIMEOUT_ENV, "300000");
    assert_eq!(bash_timeout_sec_from_env(), 300);

    let bridge = RecordingBridge::with_replies(vec![Ok((true, "partial output\n".into()))]);
    let out = env_of().exec(&bridge, "sleep 999");
    assert_eq!(bridge.calls()[0].2, 300, "桥接超时未跟随配置");
    assert!(
        out.contains("exceeding the 300s timeout"),
        "文案秒数未跟随实际值: {out}"
    );
    assert!(
        !out.contains("exceeding the 180s timeout"),
        "文案仍写死 180s"
    );
}

/// L3'-6：超上限的配置被夹到 600s（配置面也不许突破护栏）。
#[test]
fn l3prime_configured_over_cap_is_clamped_end_to_end() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var(BASH_TIMEOUT_ENV, "3600000");
    assert_eq!(bash_timeout_sec_from_env(), 600);

    let bridge = RecordingBridge::with_replies(vec![Ok((true, "partial output\n".into()))]);
    let out = env_of().exec(&bridge, "sleep 999");
    assert_eq!(bridge.calls()[0].2, 600, "超上限未被夹到 600s");
    assert!(
        out.contains("exceeding the 600s timeout"),
        "文案秒数不是夹取后的 600s: {out}"
    );
}

/// L3'-7：非法配置值落缺省（模型/运维写错参数不放大资源占用）。
#[test]
fn l3prime_invalid_config_falls_back_to_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    for raw in ["", "abc", "-5", "1.5", "  "] {
        std::env::set_var(BASH_TIMEOUT_ENV, raw);
        assert_eq!(bash_timeout_sec_from_env(), 180, "非法值 {raw:?} 未落缺省");
    }
    std::env::remove_var(BASH_TIMEOUT_ENV);
    assert_eq!(bash_timeout_sec_from_env(), 180);
}

// ============================ L3''：judge 退避重试 ============================

/// L3''-1：**终态结果不重试**（判分结论本身重试即改判分语义）。
#[test]
fn l3second_terminal_result_is_not_retried() {
    let mut attempts = 0u32;
    let mut waits: Vec<u64> = Vec::new();
    let outcome = judge_call_with_retry(
        JUDGE_MAX_ATTEMPTS,
        &JUDGE_BACKOFF_SEC,
        |_| {
            attempts += 1;
            JudgeAttempt::Terminal {
                resolved: Some(false),
                note: "judge: {\"resolved\": false}".into(),
            }
        },
        |secs| waits.push(secs),
    );
    assert_eq!(attempts, 1, "终态被判分结论被重试了");
    assert!(waits.is_empty(), "终态不该退避等待: {waits:?}");
    assert_eq!(
        outcome,
        JudgeCallOutcome::Terminal {
            resolved: Some(false),
            note: "judge: {\"resolved\": false}".into(),
        }
    );
}

/// L3''-1b：终态且 resolved=None（判分器没给结论）**也是终态**——不重试。
#[test]
fn l3second_terminal_without_resolved_is_not_retried() {
    let mut attempts = 0u32;
    let outcome = judge_call_with_retry(
        JUDGE_MAX_ATTEMPTS,
        &JUDGE_BACKOFF_SEC,
        |_| {
            attempts += 1;
            JudgeAttempt::Terminal {
                resolved: None,
                note: "judge: {}".into(),
            }
        },
        |_| {},
    );
    assert_eq!(attempts, 1, "resolved=None 的终态被重试了");
    assert!(matches!(
        outcome,
        JudgeCallOutcome::Terminal { resolved: None, .. }
    ));
}

/// L3''-2：**瞬态错误重试并退避**（3 次 / 5s·15s）。
#[test]
fn l3second_transient_error_retries_with_backoff() {
    assert_eq!(JUDGE_MAX_ATTEMPTS, 3);
    assert_eq!(JUDGE_BACKOFF_SEC, [5, 15]);

    let mut attempts = 0u32;
    let mut waits: Vec<u64> = Vec::new();
    let outcome = judge_call_with_retry(
        JUDGE_MAX_ATTEMPTS,
        &JUDGE_BACKOFF_SEC,
        |_| {
            attempts += 1;
            if attempts < 3 {
                JudgeAttempt::Transient {
                    err: format!("judge 退出 1: 第 {attempts} 次瞬态失败"),
                }
            } else {
                JudgeAttempt::Terminal {
                    resolved: Some(true),
                    note: "judge: {\"resolved\": true}".into(),
                }
            }
        },
        |secs| waits.push(secs),
    );
    assert_eq!(attempts, 3, "瞬态失败未按 3 次重试");
    assert_eq!(waits, vec![5, 15], "退避序列不是 5s·15s");
    assert_eq!(
        outcome,
        JudgeCallOutcome::Terminal {
            resolved: Some(true),
            note: "judge: {\"resolved\": true}".into(),
        }
    );
}

/// L3''-3：瞬态失败耗尽 → 末次错误原文（`JUDGE_ERROR:` 前缀由调用方按 fix3 口径加）。
#[test]
fn l3second_transient_exhausted_returns_last_error() {
    let mut attempts = 0u32;
    let mut waits: Vec<u64> = Vec::new();
    let outcome = judge_call_with_retry(
        JUDGE_MAX_ATTEMPTS,
        &JUDGE_BACKOFF_SEC,
        |_| {
            attempts += 1;
            JudgeAttempt::Transient {
                err: format!("judge 启动失败: 第 {attempts} 次"),
            }
        },
        |secs| waits.push(secs),
    );
    assert_eq!(attempts, 3);
    assert_eq!(waits, vec![5, 15]);
    assert_eq!(
        outcome,
        JudgeCallOutcome::Exhausted {
            err: "judge 启动失败: 第 3 次".into()
        }
    );
    // 生产口径的前缀（fix3 run_judge 末行）。
    let JudgeCallOutcome::Exhausted { err } = &outcome else {
        panic!("期望 Exhausted");
    };
    assert_eq!(
        format!("JUDGE_ERROR: {err}"),
        "JUDGE_ERROR: judge 启动失败: 第 3 次"
    );
}

/// L3''-4：与 fix3 `run_judge` 的循环骨架逐条等价（同输入同输出）。
///
/// 对照实现抄自 fix3 `b3e1f56` `crates/af-bench/src/judge.rs:245-304`（进程调用
/// 换成注入的 attempt，等待换成记录器；循环体、计数、退避表、末行前缀一字不改）。
fn fix3_run_judge_skeleton<A, W>(mut attempt: A, mut wait: W) -> (Option<bool>, String)
where
    A: FnMut(u32) -> Result<(Option<bool>, String), String>,
    W: FnMut(u64),
{
    const JUDGE_MAX_ATTEMPTS: u32 = 3;
    const JUDGE_BACKOFF_SEC: [u64; 2] = [5, 15];
    let mut last_err = String::new();
    for attempt_no in 1..=JUDGE_MAX_ATTEMPTS {
        match attempt(attempt_no) {
            Ok((resolved, note)) => return (resolved, note),
            Err(err) => {
                last_err = err;
            }
        }
        if attempt_no < JUDGE_MAX_ATTEMPTS {
            let wait_sec = JUDGE_BACKOFF_SEC[(attempt_no - 1) as usize];
            eprintln!(
                "[judge] 判分器第 {attempt_no}/{JUDGE_MAX_ATTEMPTS} 次失败（{last_err}），{wait_sec}s 后重试"
            );
            wait(wait_sec);
        }
    }
    (None, format!("JUDGE_ERROR: {last_err}"))
}

#[test]
fn l3second_matches_fix3_run_judge_clause_by_clause() {
    /// 失败脚本：前 k 次瞬态失败，其后终态。
    fn script(fail_times: u32) -> impl FnMut(u32) -> Result<(Option<bool>, String), String> {
        let mut seen = 0u32;
        move |_no: u32| {
            seen += 1;
            if seen <= fail_times {
                Err(format!("judge 退出 1: 瞬态 {seen}"))
            } else {
                Ok((Some(true), "judge: {\"resolved\": true}".to_string()))
            }
        }
    }

    for fail_times in 0..=3u32 {
        // fix3 基准
        let mut fix3_attempts = 0u32;
        let mut fix3_waits: Vec<u64> = Vec::new();
        let mut fix3_script = script(fail_times);
        let fix3_out = fix3_run_judge_skeleton(
            |_no| {
                fix3_attempts += 1;
                fix3_script(fix3_attempts)
            },
            |secs| fix3_waits.push(secs),
        );

        // 本实现
        let mut mine_attempts = 0u32;
        let mut mine_waits: Vec<u64> = Vec::new();
        let mine_out = judge_call_with_retry(
            JUDGE_MAX_ATTEMPTS,
            &JUDGE_BACKOFF_SEC,
            |_no| {
                mine_attempts += 1;
                // 用同一 fail_times 语义重放：前 fail_times 次失败。
                if mine_attempts <= fail_times {
                    JudgeAttempt::Transient {
                        err: format!("judge 退出 1: 瞬态 {mine_attempts}"),
                    }
                } else {
                    JudgeAttempt::Terminal {
                        resolved: Some(true),
                        note: "judge: {\"resolved\": true}".into(),
                    }
                }
            },
            |secs| mine_waits.push(secs),
        );

        assert_eq!(
            mine_attempts, fix3_attempts,
            "尝试次数不等价 (fail={fail_times})"
        );
        assert_eq!(mine_waits, fix3_waits, "退避序列不等价 (fail={fail_times})");
        let mine_tuple = match mine_out {
            JudgeCallOutcome::Terminal { resolved, note } => (resolved, note),
            JudgeCallOutcome::Exhausted { err } => (None, format!("JUDGE_ERROR: {err}")),
        };
        assert_eq!(mine_tuple, fix3_out, "终值不等价 (fail={fail_times})");
    }
}

/// L3''-5：缺省（配置面未开）= 单次调用、零等待、无 `JUDGE_ERROR:` 前缀（C 门）。
#[test]
fn l3second_default_is_single_call_without_wait() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var(JUDGE_RETRY_ENV);
    assert!(!judge_retry_enabled(), "缺省不得开启重试");
    assert!(!judge_retry_enabled_from(None));
    assert!(!judge_retry_enabled_from(Some("")));
    assert!(!judge_retry_enabled_from(Some("0")));
    assert!(!judge_retry_enabled_from(Some("off")));
    assert!(judge_retry_enabled_from(Some("1")));
    assert!(judge_retry_enabled_from(Some("true")));
    assert!(judge_retry_enabled_from(Some("YES")));

    // 缺省生产臂 = 同一驱动跑 1 次：不等待、不重试、末次错误原文（不带前缀）。
    let mut attempts = 0u32;
    let mut waits: Vec<u64> = Vec::new();
    let outcome = judge_call_with_retry(
        1,
        &[],
        |_| {
            attempts += 1;
            JudgeAttempt::Transient {
                err: "judge 启动失败: boom".into(),
            }
        },
        |secs| waits.push(secs),
    );
    assert_eq!(attempts, 1, "缺省臂不是单次调用");
    assert!(waits.is_empty(), "缺省臂不该等待: {waits:?}");
    assert_eq!(
        outcome,
        JudgeCallOutcome::Exhausted {
            err: "judge 启动失败: boom".into()
        }
    );
}
