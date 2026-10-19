//! line-loader —— CUE→JSON→内核装载端到端的装载器样例。
//!
//! 用法：
//!   line-loader <bundle.json>            装载；成功打摘要 JSON（exit 0），失败打结构化错误（exit 2）
//!   line-loader --bench <bundle.json> N  装载 N 次计时，报 min/p50/mean（毫秒）

use line_loader::{load_str, LoadError};
use std::env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: line-loader <bundle.json> | line-loader --bench <bundle.json> <N>");
        std::process::exit(64);
    }
    if args[1] == "--bench" {
        if args.len() < 4 {
            eprintln!("用法: line-loader --bench <bundle.json> <N>");
            std::process::exit(64);
        }
        bench(&args[2], args[3].parse().expect("N 必须是整数"));
        return;
    }
    let text = fs::read_to_string(&args[1]).unwrap_or_else(|e| {
        eprintln!("读文件失败 {0}: {e}", args[1]);
        std::process::exit(64);
    });
    match load_str(&text) {
        Ok(b) => {
            println!(
                "{{\"ok\":true,\"schemaVersion\":{},\"line\":\"{}\",\"rules\":{},\"andonRules\":{},\"tools\":{},\"evalFunctions\":{}}}",
                b.schema_version,
                b.line.id,
                b.rules.len(),
                b.line.andon_rules.len(),
                b.line.tools.len(),
                b.line.eval_functions.len(),
            );
            std::process::exit(0);
        }
        Err(e) => {
            println!(
                "{{\"ok\":false,\"code\":\"{}\",\"kind\":\"{}\",\"detail\":\"{}\"}}",
                e.code(), e.kind_name(), e.to_string().replace('"', "'")
            );
            std::process::exit(2);
        }
    }
}

fn bench(path: &str, n: usize) {
    let text = fs::read_to_string(path).expect("读文件失败");
    let mut ts = Vec::new();
    for _ in 0..n {
        let t0 = Instant::now();
        let r = load_str(&text);
        ts.push(t0.elapsed().as_secs_f64() * 1000.0);
        if let Err(e) = r {
            eprintln!("装载失败: {} ({})", e.code(), e);
            std::process::exit(2);
        }
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = ts[n / 2];
    let min = ts[0];
    let mean = ts.iter().sum::<f64>() / n as f64;
    println!("n={n} bytes={} min={min:.3}ms p50={p50:.3}ms mean={mean:.3}ms", text.len());
}

#[allow(dead_code)]
fn _assert_error_is_std(e: &LoadError) {
    let _: &dyn std::error::Error = e;
}
