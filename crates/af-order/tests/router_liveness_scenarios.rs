//! 收件箱活性安灯四场景验收（风险#7：建成没事件也要报活）——af 产品链
//! （Rust 面）读数腿 [`af_order::router_liveness`] 的定点复算。判定语义与
//! PoC-D `tools/event-router.py` `_trigger_liveness_andon()` 逐字对齐（对齐
//! 正本镜像 = `poc/poc-super-agent/tests/event-router-poCD.diff`）；本文件
//! 用钉死的 `now` 复算四场景 + 锚点生命周期 + 只读性对拍 + 边界语义。
//! Rust↔Python 两腿同场景对拍在 `poc/poc-risk7-liveness/`（证据目录）。

use std::fs;
use std::path::{Path, PathBuf};

use af_order::router_liveness::{
    evaluate_liveness, write_snapshot, AnchorSource, LivenessError, LivenessStatus,
};

/// 钉死锚点时刻（所有确定性场景共用；f64 精确表示）。
const T0: f64 = 1_700_000_000.0;

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "af-order-liveness-it-{}-{}-{}",
        std::process::id(),
        tag,
        nanos
    ));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

/// 造运行位：注册表（liveness 段可配）+ 可选收件箱投递文件。
/// 返回 triggers home。
fn home_with(
    root: &Path,
    tag: &str,
    liveness_json: Option<&str>,
    deliveries: &[(&str, &str)], // (source_dir, file_name)
) -> PathBuf {
    let home = root.join(tag);
    fs::create_dir_all(home.join("inbox")).unwrap();
    let liveness = liveness_json.unwrap_or("");
    fs::write(
        home.join("registry.json"),
        format!(
            r#"{{"schema":"af.trigger.registry.v1","sources":[{{"name":"mes-execline",
                "domain":"mes","events":["order_complete"]}}]{liveness}}}"#
        ),
    )
    .unwrap();
    for (source, file) in deliveries {
        let dir = home.join("inbox").join(source);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file), r#"{"event":"order_complete"}"#).unwrap();
    }
    home
}

fn mtime_of(path: &Path) -> f64 {
    path.metadata()
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

/// 递归清单（相对路径集合——只读性对拍用；mtime 不进清单，锚点新建不改别件）。
fn inventory(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let rel = entry
                .path()
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if entry.path().is_dir() {
                out.push(format!("{rel}/"));
                walk(base, &entry.path(), out);
            } else {
                out.push(rel);
            }
        }
    }
    walk(root, root, &mut out);
    out.sort();
    out
}

/// 场景①：收件箱有近投递 ⇒ green（起算=最新投递 mtime，非锚点）。
#[test]
fn scenario1_recent_delivery_is_green() {
    let root = scratch("s1");
    let home = home_with(&root, "home", None, &[("mes-execline", "d-1.json")]);
    let delivery = home.join("inbox/mes-execline/d-1.json");
    let m = mtime_of(&delivery);
    // 腿先起跑（锚点=过去 T0），投递在其后 ⇒ since = 最新投递 mtime。
    evaluate_liveness(&home, T0).unwrap();
    let reading = evaluate_liveness(&home, m + 120.0).unwrap();
    assert_eq!(reading.status, LivenessStatus::Green, "近投递不报警");
    assert_eq!(reading.threshold_min, 240, "缺 liveness 段回落 240");
    assert_eq!(reading.anchor_source, AnchorSource::NewestDelivery);
    assert!(
        (reading.silent_s - 120.0).abs() < 1.0,
        "静默秒数可复算（now−最新投递 mtime）：{}",
        reading.silent_s
    );
    assert_eq!(reading.inbox_dir, home.join("inbox"));
    let _ = fs::remove_dir_all(&root);
}

/// 场景②：收件箱空但刚起跑 ⇒ green（起算=本腿首跑锚点，不冤枉刚部署）。
#[test]
fn scenario2_empty_inbox_just_started_is_green() {
    let root = scratch("s2");
    let home = home_with(&root, "home", None, &[]);
    let reading = evaluate_liveness(&home, T0).unwrap();
    assert!(reading.anchor_created, "首跑创建锚点");
    assert_eq!(reading.status, LivenessStatus::Green, "刚部署不报警");
    assert_eq!(reading.anchor_source, AnchorSource::FirstRunAnchor);
    assert_eq!(reading.silent_s, 0.0, "起算=当次 now");
    assert_eq!(reading.since_epoch_s, T0);
    let _ = fs::remove_dir_all(&root);
}

/// 场景③：收件箱空且锚点已久 ⇒ amber（静默秒数可复算、来源=首跑锚点、
/// 锚点复用不刷新）。
#[test]
fn scenario3_stale_anchor_is_amber_and_anchor_not_refreshed() {
    let root = scratch("s3");
    let home = home_with(&root, "home", None, &[]);
    evaluate_liveness(&home, T0).unwrap(); // 首跑写锚点 T0
    let reading = evaluate_liveness(&home, T0 + 251.0 * 60.0).unwrap();
    assert_eq!(reading.status, LivenessStatus::Amber, "251min > 240min 阈");
    assert_eq!(reading.threshold_min, 240);
    assert_eq!(reading.anchor_source, AnchorSource::FirstRunAnchor);
    assert_eq!(reading.silent_s, 251.0 * 60.0, "静默秒数可复算");
    assert_eq!(reading.since_epoch_s, T0, "锚点复用不刷新");
    assert!(!reading.anchor_created);
    assert_eq!(reading.alarm_code(), "trigger-inbox-silent");
    // 锚点文件内容仍是 T0（后续跑不改写）。
    let anchor_text = fs::read_to_string(home.join("liveness-anchor.json")).unwrap();
    assert!(
        anchor_text.contains("1700000000"),
        "锚点文件保持首跑值：{anchor_text}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// 场景④：liveness 段缺失 ⇒ 回落 240（显式 None 判）；阈值 0 ⇒ 立即
/// amber（证明 0 不是被当缺省吞掉）；恰好压线（silent == 阈×60）⇒ green
/// （严格大于，与 PoC-D `>` 同款）。
#[test]
fn scenario4_threshold_fallback_and_zero_is_legal() {
    let root = scratch("s4");
    // ④a：无 liveness 段 → 240；239min green / 恰 240min green（严格 >）/241min amber。
    let home = home_with(&root, "no-liveness", None, &[]);
    evaluate_liveness(&home, T0).unwrap();
    let green = evaluate_liveness(&home, T0 + 239.0 * 60.0).unwrap();
    assert_eq!(
        (green.status, green.threshold_min),
        (LivenessStatus::Green, 240)
    );
    let on_line = evaluate_liveness(&home, T0 + 240.0 * 60.0).unwrap();
    assert_eq!(
        on_line.status,
        LivenessStatus::Green,
        "silent==阈×60 不 amber（严格 >）"
    );
    let over = evaluate_liveness(&home, T0 + 241.0 * 60.0).unwrap();
    assert_eq!(
        over.status,
        LivenessStatus::Amber,
        "241min > 240min ⇒ amber"
    );
    // ④b：阈值 0 → 立即 amber（Some(0) 不是 None，不被缺省吞）。
    let zero = home_with(
        &root,
        "zero-threshold",
        Some(r#","liveness":{"inbox_silent_after_min":0}"#),
        &[],
    );
    evaluate_liveness(&zero, T0).unwrap();
    let reading = evaluate_liveness(&zero, T0 + 2.0).unwrap();
    assert_eq!(
        (reading.status, reading.threshold_min),
        (LivenessStatus::Amber, 0)
    );
    assert_eq!(reading.silent_s, 2.0, "静默 2s 即 amber（阈 0）");
    // 压线补充：阈 0 且 silent 恰 0（now==锚点）⇒ 0 > 0 不成立 ⇒ green。
    let zero_edge = home_with(
        &root,
        "zero-edge",
        Some(r#","liveness":{"inbox_silent_after_min":0}"#),
        &[],
    );
    let edge = evaluate_liveness(&zero_edge, T0).unwrap();
    assert_eq!(
        edge.status,
        LivenessStatus::Green,
        "silent==0 且阈==0：0>0 假 ⇒ green"
    );
    let _ = fs::remove_dir_all(&root);
}

/// 锚点生命周期：首跑创建 → 后续复用 → 损坏（非 JSON / 负值）fail-closed
/// 拒静默重建（静默时钟不清零）→ 显式删锚点后重新起算（知情重置）。
#[test]
fn anchor_lifecycle_corrupt_is_fail_closed() {
    let root = scratch("anchor");
    let home = home_with(&root, "home", None, &[]);
    assert!(evaluate_liveness(&home, T0).unwrap().anchor_created);
    // 复用：第二跑不新建（anchor_created=false）且锚点值不变。
    let second = evaluate_liveness(&home, T0 + 60.0).unwrap();
    assert!(!second.anchor_created);
    assert_eq!(second.since_epoch_s, T0);
    // 损坏：非 JSON。
    fs::write(home.join("liveness-anchor.json"), "not-json").unwrap();
    let err = evaluate_liveness(&home, T0 + 120.0).unwrap_err();
    assert!(matches!(err, LivenessError::Anchor(_)), "{err}");
    // 损坏：负值（坏形）。
    fs::write(
        home.join("liveness-anchor.json"),
        r#"{"first_run_epoch_s":-5}"#,
    )
    .unwrap();
    assert!(matches!(
        evaluate_liveness(&home, T0 + 120.0),
        Err(LivenessError::Anchor(_))
    ));
    // 损坏后锚点文件不被改写（fail-closed 无写面），显式删 = 重新起算。
    let anchor_path = home.join("liveness-anchor.json");
    assert_eq!(
        fs::read_to_string(&anchor_path).unwrap(),
        r#"{"first_run_epoch_s":-5}"#
    );
    fs::remove_file(&anchor_path).unwrap();
    let fresh = evaluate_liveness(&home, T0 + 180.0).unwrap();
    assert_eq!(
        fresh.since_epoch_s,
        T0 + 180.0,
        "删锚点后从当次 now 重新起算"
    );
    let _ = fs::remove_dir_all(&root);
}

/// 注册表面 fail-closed：注册表缺席 / schema 不识别 ⇒ 不产读数（PoC-D
/// 消费腿早退同口径），不假绿。
#[test]
fn registry_gate_is_fail_closed() {
    let root = scratch("regate");
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    // 缺席。
    assert!(matches!(
        evaluate_liveness(&home, T0),
        Err(LivenessError::Registry(_))
    ));
    // schema 不识别。
    fs::write(
        home.join("registry.json"),
        r#"{"schema":"af.trigger.registry.v2"}"#,
    )
    .unwrap();
    assert!(matches!(
        evaluate_liveness(&home, T0),
        Err(LivenessError::Registry(_))
    ));
    // 坏 JSON。
    fs::write(home.join("registry.json"), "not-json").unwrap();
    assert!(matches!(
        evaluate_liveness(&home, T0),
        Err(LivenessError::Registry(_))
    ));
    let _ = fs::remove_dir_all(&root);
}

/// 收件箱扫描 = glob `inbox/*/*.json` 语义镜像：点开头名（文件与 source
/// 目录两级）不计入，`.json` 后缀外不计入。
#[test]
fn inbox_scan_mirrors_glob_semantics() {
    let root = scratch("glob");
    let home = home_with(&root, "home", None, &[("mes-execline", "keep.json")]);
    let keep = home.join("inbox/mes-execline/keep.json");
    let m = mtime_of(&keep);
    // 点开头文件与点开头 source 目录都更新——若被计入，since 会被抬高。
    fs::write(home.join("inbox/mes-execline/.hidden.json"), "{}").unwrap();
    fs::create_dir_all(home.join("inbox/.hidden-src")).unwrap();
    fs::write(home.join("inbox/.hidden-src/newer.json"), "{}").unwrap();
    fs::write(home.join("inbox/mes-execline/notes.txt"), "{}").unwrap();
    evaluate_liveness(&home, T0).unwrap();
    let reading = evaluate_liveness(&home, m + 60.0).unwrap();
    assert!(
        (reading.silent_s - 60.0).abs() < 1.0,
        "静默按 keep.json mtime 复算（点开头/非 .json 不计）：{}",
        reading.silent_s
    );
    assert_eq!(reading.anchor_source, AnchorSource::NewestDelivery);
    let _ = fs::remove_dir_all(&root);
}

/// 只读性对拍：判定 + 快照落盘前后，triggers home 内唯一新增文件 =
/// liveness-anchor.json；consumed.jsonl / dispatch / 账本 / 派单面零出现。
#[test]
fn liveness_leg_writes_only_the_anchor_file() {
    let root = scratch("readonly");
    let home = home_with(&root, "home", None, &[("mes-execline", "d-1.json")]);
    let out = root.join("out").join("trigger-inbox-liveness.json");
    let before = inventory(&root);
    let reading = evaluate_liveness(&home, T0).unwrap();
    write_snapshot(&reading, &out).unwrap();
    let after = inventory(&root);
    let added: Vec<String> = after
        .iter()
        .filter(|p| !before.contains(*p))
        .cloned()
        .collect();
    assert_eq!(
        added,
        vec![
            "home/liveness-anchor.json".to_string(),
            "out/".to_string(),
            "out/trigger-inbox-liveness.json".to_string(),
        ],
        "全盘新增仅两处：home 内唯一新增=锚点；home 外唯一新增=调用方指定快照"
    );
    assert!(!home.join("consumed.jsonl").exists(), "不写消费游标");
    assert!(!home.join("dispatch").exists(), "不派单");
    assert!(!root.join("orders").exists(), "不造账本");
    // 快照内容 = 10 字段同序 + 不残留 .tmp。
    let text = fs::read_to_string(&out).unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&str> = snapshot
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "ts",
            "ts_epoch",
            "tool",
            "status",
            "message",
            "alarm_code",
            "newest_delivery_age_s",
            "threshold_min",
            "inbox_dir",
            "since_epoch",
        ]
    );
    assert!(
        !root.join("out/trigger-inbox-liveness.json.tmp").exists(),
        "tmp 已换名"
    );
    // 二次写 = 覆盖更新（每轮重写=报活心跳），仍无第三文件。
    let second = evaluate_liveness(&home, T0 + 60.0).unwrap();
    write_snapshot(&second, &out).unwrap();
    assert_eq!(inventory(&root), after, "重写快照不新增文件");
    let _ = fs::remove_dir_all(&root);
}
