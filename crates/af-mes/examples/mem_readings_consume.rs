//! J2 消费腿可执行面（可机读消费证据 runner）：
//! 读一份 memory-readings.jsonl，把最新三读数 + 门裁决打成一个 JSON。
//!
//! 用法:
//!   mem_readings_consume <memory-readings.jsonl> \
//!       [min_mem_available_kb] [max_swap_used_kb] [max_tmpfs_used_b] [--line <line_id>]
//!
//! 阈值与 line_id 均由调用方注入；方括号内缺省值只是本机防线预算的示例初值，
//! 不等于裁决。退出码对齐 ci 门口径：0=绿、1=越线、2=不可证/用法错。

use af_mes::{MemDefenseGate, MemDefenseVerdict, MemReadingsLog};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!(
            "用法: mem_readings_consume <memory-readings.jsonl> \
             [min_mem_available_kb] [max_swap_used_kb] [max_tmpfs_used_b] [--line <line_id>]"
        );
        return ExitCode::from(2);
    };
    let mut nums = Vec::new();
    let mut line_id = "factory-host".to_string();
    while let Some(a) = args.next() {
        if a == "--line" {
            line_id = args.next().unwrap_or_else(|| line_id.clone());
        } else {
            nums.push(a);
        }
    }
    let num = |i: usize| nums.get(i).and_then(|s| s.parse::<u64>().ok());
    let gate = MemDefenseGate {
        min_mem_available_kb: num(0).unwrap_or(1_000_000),
        max_swap_used_kb: num(1).unwrap_or(16_000_000),
        max_tmpfs_used_b: num(2).unwrap_or(5_368_709_120),
    };

    let log = match MemReadingsLog::open(&path) {
        Ok(log) => log,
        Err(err) => {
            // fail-closed：不可证 ⇒ rc=2，绝不猜绿。
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    let latest = match log.latest() {
        Ok(row) => row,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    let verdict = gate.judge(latest);
    let (pass, breaches): (bool, Vec<String>) = match &verdict {
        MemDefenseVerdict::Pass => (true, Vec::new()),
        MemDefenseVerdict::Breach(b) => (false, b.clone()),
    };
    let out = serde_json::json!({
        "path": log.path(),
        "parseable_rows": log.rows().len(),
        "bad_lines": log.bad_lines().len(),
        "latest": latest,
        "gate": {
            "min_mem_available_kb": gate.min_mem_available_kb,
            "max_swap_used_kb": gate.max_swap_used_kb,
            "max_tmpfs_used_b": gate.max_tmpfs_used_b,
        },
        "verdict": { "pass": pass, "breaches": breaches },
        "events": latest.to_events(&line_id),
    });
    println!("{out}");
    if pass {
        ExitCode::from(0)
    } else {
        ExitCode::from(1)
    }
}
