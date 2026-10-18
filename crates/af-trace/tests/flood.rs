//! 洪峰验证（PoC 指定检查）：
//! 10 万 normal 事件多线程灌入 + 慢消费者（200µs/条），穿插 100 条 critical 检查点
//! （fail-closed 重试模式）。验证：
//! 1. critical 全部落盘零丢失（所有 Ok 的 seq 都能在 critical sink 中找到）；
//! 2. gap_counter == 实际丢弃数（发送数 - 落盘数）；
//! 3. Σ(gap span.count) == gap_counter，“缺了多少”成为落盘里的可验证事实。

use af_trace::{Level, Trace, TraceConfig};
use serde_json::json;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const NORMAL_TOTAL: usize = 100_000;
const PRODUCERS: usize = 4;
const CHECKPOINTS: usize = 100;

fn parse_sink(sink: &Mutex<Vec<String>>) -> Vec<af_trace::TraceEvent> {
    sink.lock()
        .unwrap()
        .iter()
        .map(|l| serde_json::from_str(l).expect("sink line 必须是合法 JSON TraceEvent"))
        .collect()
}

#[test]
fn flood_100k_normal_events() {
    let trace = Trace::spawn(TraceConfig {
        critical_capacity: 8192,
        normal_capacity: 512,      // 小队列 → 洪峰必然溢出
        critical_consume_delay_us: 0, // critical 落盘不拖后腿
        normal_consume_delay_us: 200, // normal 慢消费者：200µs/条
    });

    let ok_critical: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let rejected_critical: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let retries = Arc::new(AtomicU64::new(0));

    // ---- critical 写入线程：fail-closed + 重试，直到成功 ----
    let crit_writer = {
        let trace = trace.clone();
        let ok_critical = ok_critical.clone();
        let rejected_critical = rejected_critical.clone();
        let retries = retries.clone();
        thread::Builder::new()
            .name("critical-writer".into())
            .spawn(move || {
                for i in 0..CHECKPOINTS {
                    loop {
                        match trace.write_critical("checkpoint", json!({ "i": i })) {
                            Ok(seq) => {
                                ok_critical.lock().unwrap().push(seq);
                                break;
                            }
                            Err(e) => {
                                // fail-closed：调用方必须处理——这里选择记录并退避重试
                                rejected_critical.lock().unwrap().push(e.event().seq);
                                retries.fetch_add(1, Ordering::Relaxed);
                                thread::sleep(Duration::from_millis(1));
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            })
            .expect("spawn critical writer")
    };

    // ---- 4 个 normal 生产者线程，共 10 万事件 ----
    let per_thread = NORMAL_TOTAL / PRODUCERS;
    let mut producers = Vec::new();
    for p in 0..PRODUCERS {
        let trace = trace.clone();
        producers.push(
            thread::Builder::new()
                .name(format!("normal-producer-{p}"))
                .spawn(move || {
                    for i in 0..per_thread {
                        trace.write_normal("agent.step", json!({ "producer": p, "i": i }));
                    }
                })
                .expect("spawn producer"),
        );
    }

    for h in producers {
        h.join().unwrap();
    }
    crit_writer.join().unwrap();

    let gap_at_quiesce = trace.gap_counter();
    let report = trace.shutdown();

    // ============ 验证 1：critical 全部落盘、零丢失 ============
    let crit = parse_sink(&report.sinks.critical);
    let crit_seqs: HashSet<u64> = crit.iter().map(|e| e.seq).collect();
    assert_eq!(
        crit.len(),
        crit_seqs.len(),
        "critical sink 内不允许重复 seq"
    );
    let ok_seqs = ok_critical.lock().unwrap().clone();
    assert_eq!(ok_seqs.len(), CHECKPOINTS, "100 条检查点经重试后全部成功");
    for seq in &ok_seqs {
        assert!(crit_seqs.contains(seq), "Ok(seq={seq}) 的 critical 事件必须落盘");
    }
    assert!(
        crit.iter().all(|e| e.level == Level::Critical),
        "critical sink 只应含 critical 级别事件"
    );

    // ============ 验证 2：gap 计数与实际丢弃一致 ============
    let delivered = report.normal_persisted as u64;
    let dropped = NORMAL_TOTAL as u64 - delivered;
    assert_eq!(
        report.gap_counter, dropped,
        "gap_counter 必须 == 发送数 - 实际落盘数"
    );
    assert_eq!(gap_at_quiesce, report.gap_counter, "生产者结束后不应再新增丢弃");
    assert!(dropped >= 1_000, "洪峰必须真实压垮 512 容量的队列（丢弃 {dropped}）");
    assert!(delivered >= 1, "慢消费者仍应消费掉一部分事件");

    // ============ 验证 3：每个缺口都成为落盘的 critical 事实 ============
    assert_eq!(
        report.gap_reported, report.gap_counter,
        "shutdown 补发后 reported == counter"
    );
    let gap_sum: u64 = crit.iter().filter_map(|e| e.gap.as_ref()).map(|g| g.count).sum();
    assert_eq!(gap_sum, report.gap_counter, "Σ(gap span.count) == 实际丢弃数");
    let gap_spans = crit.iter().filter(|e| e.kind == "gap").count();
    assert!(gap_spans >= 1, "至少一条 gap span");

    // ============ normal 侧无重复、全为 normal 级别 ============
    let normal = parse_sink(&report.sinks.normal);
    let normal_seqs: HashSet<u64> = normal.iter().map(|e| e.seq).collect();
    assert_eq!(normal.len(), normal_seqs.len(), "normal sink 不允许重复");
    assert_eq!(normal.len() as u64, delivered);
    assert!(normal.iter().all(|e| e.level == Level::Normal));

    let rejected = rejected_critical.lock().unwrap().clone();
    println!("==== 洪峰结果 ====");
    println!("normal 发送: {NORMAL_TOTAL}, 落盘: {delivered}, 丢弃: {dropped} ({:.2}%)",
        dropped as f64 / NORMAL_TOTAL as f64 * 100.0);
    println!("gap_counter == gap_reported == {gap_sum}（gap span 条数: {gap_spans}）");
    println!("critical 落盘: {} 行（检查点 {} + gap span {gap_spans}），重试次数: {}（被拒 seq 数: {}）",
        report.critical_persisted, CHECKPOINTS, retries.load(Ordering::Relaxed), rejected.len());
}
