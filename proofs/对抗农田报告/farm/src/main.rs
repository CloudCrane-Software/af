//! afv1-farm：对抗样本农田 runner。只读：不改草稿、不改 af-dsl。
//!
//! 用法：
//!   afv1-farm one <file.yaml>     单样本裁定（子进程模式，父进程带超时调用）
//!   afv1-farm run <dir> <out.tsv> [concurrency]  批量：每样本 spawn 一个 `one` 子进程
//!
//! 裁定形态（TSV 一行，字段制表符分隔）：
//!   file \t verdict \t layer \t codes \t nvio \t panic \t ms
//! verdict ∈ PASS | REJECT | PANIC-CAUGHT | CRASH-SIGNAL | TIMEOUT | IO-ERROR
//! panic 字段：`-`（无）或 catch_unwind 捕获的 panic 载荷摘要（换行转义）。

use af_dsl::{load_str, validate};
use std::io::Write as _;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("one") => one(&args[1]),
        Some("detail") => detail(&args[1]),
        Some("run") => {
            let conc: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(8);
            run(&args[1], &args[2], conc);
        }
        _ => {
            eprintln!("usage: afv1-farm one <file> | detail <file> | run <dir> <out.tsv> [conc]");
            std::process::exit(2);
        }
    }
}

/// 复现调查模式：g3gate 同款逐条违例输出（供报告引用）。
fn detail(path: &str) {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            println!("IO-ERROR {e}");
            return;
        }
    };
    match load_str(&src) {
        Err(e) => {
            let layer = match &e {
                af_dsl::LoadError::Envelope(_) => "envelope",
                af_dsl::LoadError::Shape(_) => "shape",
                af_dsl::LoadError::Semantic(_) => "semantic",
            };
            println!("VERDICT REJECT layer={layer}");
            for v in &e.report().violations {
                println!("  [{}] {} [{}] {}", v.code.code(), v.code.name(), v.scope, v.message);
            }
        }
        Ok(doc) => {
            let rep = validate(&doc);
            if rep.is_ok() {
                println!("VERDICT PASS");
            } else {
                println!("VERDICT REJECT layer=semantic");
                for v in rep.violations {
                    println!("  [{}] {} [{}] {}", v.code.code(), v.code.name(), v.scope, v.message);
                }
            }
        }
    }
}

/// 单样本：本进程内 catch_unwind 兜底 + 结构化输出。
fn one(path: &str) {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            emit(path, "IO-ERROR", "-", "-", 0, "-", 0);
            eprintln!("read: {e}");
            return;
        }
    };
    let t0 = Instant::now();

    // ① load_str：panic → PANIC-CAUGHT（载荷归一）；不 panic 才继续。
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load_str(&src)));
    let doc = match loaded {
        Err(payload) => {
            emit(
                path,
                "PANIC-CAUGHT",
                "load_str",
                "-",
                0,
                &payload_brief(&payload),
                t0.elapsed().as_millis(),
            );
            return;
        }
        Ok(Err(e)) => {
            let r = e.report();
            let layer = match &e {
                af_dsl::LoadError::Envelope(_) => "envelope",
                af_dsl::LoadError::Shape(_) => "shape",
                af_dsl::LoadError::Semantic(_) => "semantic",
            };
            emit(
                path,
                "REJECT",
                layer,
                &r.codes().join(","),
                r.violations.len(),
                "-",
                t0.elapsed().as_millis(),
            );
            return;
        }
        Ok(Ok(doc)) => doc,
    };

    // ② validate：panic → PANIC-CAUGHT（layer=validate）。
    let rep = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| validate(&doc))) {
        Err(payload) => {
            emit(
                path,
                "PANIC-CAUGHT",
                "validate",
                "-",
                0,
                &payload_brief(&payload),
                t0.elapsed().as_millis(),
            );
            return;
        }
        Ok(rep) => rep,
    };
    if rep.is_ok() {
        emit(path, "PASS", "semantic", "-", 0, "-", t0.elapsed().as_millis());
    } else {
        emit(
            path,
            "REJECT",
            "semantic",
            &rep.codes().join(","),
            rep.violations.len(),
            "-",
            t0.elapsed().as_millis(),
        );
    }
}

fn payload_brief(payload: &(dyn std::any::Any + Send)) -> String {
    let raw = if let Some(m) = payload.downcast_ref::<&str>() {
        (*m).to_string()
    } else if let Some(m) = payload.downcast_ref::<String>() {
        m.clone()
    } else {
        "non-string payload".to_string()
    };
    let mut out = String::new();
    for c in raw.chars().take(160) {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

fn emit(path: &str, verdict: &str, layer: &str, codes: &str, nvio: usize, panic: &str, ms: u128) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(
        out,
        "{path}\t{verdict}\t{layer}\t{codes}\t{nvio}\t{panic}\t{ms}"
    );
    let _ = out.flush();
}

/// 批量：每样本独立子进程（隔离栈溢出 / abort / 死循环），带 20s 超时。
fn run(dir: &str, out_tsv: &str, conc: usize) {
    let mut files: Vec<String> = Vec::new();
    for ent in std::fs::read_dir(dir).expect("read_dir") {
        let ent = ent.expect("ent");
        let p = ent.path();
        if matches!(p.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")) {
            files.push(p.to_string_lossy().into_owned());
        }
    }
    files.sort();
    let exe = std::env::current_exe().expect("current_exe");
    let sem = std::sync::Arc::new(std::sync::Mutex::new(()));
    let _ = sem; // 简化：顺序 collect，用线程池并行 spawn
    let files = std::sync::Arc::new(files);
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut handles = Vec::new();
    let next = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for _ in 0..conc {
        let files = files.clone();
        let results = results.clone();
        let next = next.clone();
        let exe = exe.clone();
        handles.push(std::thread::spawn(move || loop {
            let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let Some(f) = files.get(i) else { break };
            let line = run_one_child(&exe, f);
            results.lock().unwrap().push(line);
        }));
    }
    for h in handles {
        h.join().expect("join");
    }
    let mut all = results.lock().unwrap();
    all.sort();
    let mut w = std::io::BufWriter::new(std::fs::File::create(out_tsv).expect("create"));
    for line in all.iter() {
        writeln!(w, "{line}").unwrap();
    }
    w.flush().unwrap();
    eprintln!("farmed {} samples → {out_tsv}", all.len());
}

fn run_one_child(exe: &std::path::Path, f: &str) -> String {
    use std::process::{Command, Stdio};
    let t0 = Instant::now();
    let child = Command::new(exe)
        .arg("one")
        .arg(f)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            return format!("{f}\tIO-ERROR\tspawn\t-\t0\t{e}\t0");
        }
    };
    let timeout = Duration::from_secs(20);
    let deadline = Instant::now() + timeout;
    // 轮询等待（避免额外依赖 wait-timeout crate）。
    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(st)) => break (Some(st), false),
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break (None, true);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => {
                return format!("{f}\tIO-ERROR\twait\t-\t0\t{e}\t0");
            }
        }
    };
    let ms = t0.elapsed().as_millis();
    if timed_out {
        return format!("{f}\tTIMEOUT\tprocess\t-\t0\t-\t{ms}");
    }
    let st = status.unwrap();
    use std::os::unix::process::ExitStatusExt;
    if let Some(sig) = st.signal() {
        return format!("{f}\tCRASH-SIGNAL\tsignal-{sig}\t-\t0\t-\t{ms}");
    }
    if !st.success() {
        // `one` 内部已兜底 panic，不应有非零退出；记录原始码。
        return format!("{f}\tCRASH-SIGNAL\texit-{}\t-\t0\t-\t{ms}", st.code().unwrap_or(-1));
    }
    let mut out = String::new();
    if let Some(mut so) = child.stdout.take() {
        use std::io::Read;
        let _ = so.read_to_string(&mut out);
    }
    let line = out.lines().next().unwrap_or("").to_string();
    if line.is_empty() {
        format!("{f}\tCRASH-SIGNAL\tempty-stdout\t-\t0\t-\t{ms}")
    } else {
        line
    }
}
