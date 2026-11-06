//! cuengine FFI 实测探针
//! 1) 最小求值: price: int & >5 + 输入 42 / 3(反例)
//! 2) closed schema: definition 未知字段拒绝
//! 3) 100 次求值计时(对比 CLI 进程路线 7-17 ms/次)

use cuengine::{evaluate_cue_package, get_bridge_version};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 搭一个最小 CUE 模块目录(bridge 要求 cue.mod/module.cue)
fn make_module(root: &Path, schema: &str, input: &str) -> std::io::Result<PathBuf> {
    let dir = root.join("mod");
    fs::create_dir_all(dir.join("cue.mod"))?;
    fs::write(
        dir.join("cue.mod/module.cue"),
        "module: \"probe.local/ffi\"\nlanguage: version: \"v0.9.0\"\n",
    )?;
    fs::write(dir.join("schema.cue"), schema)?;
    fs::write(dir.join("input.cue"), input)?;
    Ok(dir)
}

fn main() {
    let work = std::env::temp_dir().join("cuengine-ffi-probe");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();

    println!("== 0. bridge 版本 ==");
    match get_bridge_version() {
        Ok(v) => println!("bridge version: {v}"),
        Err(e) => {
            eprintln!("get_bridge_version 失败: {e}");
            std::process::exit(2);
        }
    }

    println!("\n== 1. 最小求值 price: int & >5, 输入 42 ==");
    let dir = make_module(
        &work,
        "package probe\n\nprice: int & >5\n",
        "package probe\n\nprice: 42\n",
    )
    .unwrap();
    match evaluate_cue_package(&dir, "probe") {
        Ok(json) => println!("OK: {json}"),
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }

    println!("\n== 2. 反例: 输入 3 (违反 >5) ==");
    let dir_bad = make_module(
        &work,
        "package probe\n\nprice: int & >5\n",
        "package probe\n\nprice: 3\n",
    )
    .unwrap();
    match evaluate_cue_package(&dir_bad, "probe") {
        Ok(json) => {
            eprintln!("意外通过(不该发生): {json}");
            std::process::exit(1);
        }
        Err(e) => println!("预期拒绝: {e}"),
    }

    println!("\n== 3. closed schema: definition 未知字段 ==");
    let dir_closed = make_module(
        &work,
        "package probe\n\n#Rule: {\n\tkind: string\n\tparams: threshold: number\n}\n\nrule: #Rule\n",
        "package probe\n\nrule: {\n\tkind: \"latency\"\n\tparams: threshold: 120\n\towner: \"nobody\"\n}\n",
    )
    .unwrap();
    match evaluate_cue_package(&dir_closed, "probe") {
        Ok(json) => {
            eprintln!("意外通过(closed 应拒绝未知字段): {json}");
            std::process::exit(1);
        }
        Err(e) => println!("预期拒绝: {e}"),
    }

    println!("\n== 4. closed schema 正例(无未知字段) ==");
    let dir_ok = make_module(
        &work,
        "package probe\n\n#Rule: {\n\tkind: string\n\tparams: threshold: number\n}\n\nrule: #Rule\n",
        "package probe\n\nrule: {\n\tkind: \"latency\"\n\tparams: threshold: 120\n}\n",
    )
    .unwrap();
    match evaluate_cue_package(&dir_ok, "probe") {
        Ok(json) => println!("OK: {json}"),
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }

    println!("\n== 5. 100 次求值计时 ==");
    let n = 100;
    let mut ms: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let t0 = Instant::now();
        let r = evaluate_cue_package(&dir_ok, "probe");
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        if r.is_err() {
            eprintln!("第 {i} 次失败: {:?}", r.err().unwrap());
            std::process::exit(1);
        }
        ms.push(dt);
    }
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let sum: f64 = ms.iter().sum();
    println!(
        "n=100  min={:.2}ms  p50={:.2}ms  mean={:.2}ms  p95={:.2}ms  max={:.2}ms",
        ms[0],
        ms[n / 2],
        sum / n as f64,
        ms[(n as f64 * 0.95) as usize],
        ms[n - 1]
    );

    // 对照组: 同目录 cue CLI 进程路线 20 次(单独脚本计时, 更准)
    println!(
        "cli 对照目录: {}",
        dir_ok.display()
    );
}
