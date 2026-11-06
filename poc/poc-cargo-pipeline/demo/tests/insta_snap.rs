use demo::clamp_pct;
use insta::assert_debug_snapshot;

/// 快照测试：一批输入对应的 clamp 结果整体序列化后比对。
/// 输出形态关注点：diff 是结构化可读文本，insta 另出 .snap.new 待接受文件。
#[test]
fn clamp_pct_snapshot() {
    let inputs = [-7i64, -1, 0, 1, 50, 99, 100, 101, 999];
    let results: Vec<u8> = inputs.iter().copied().map(clamp_pct).collect();
    assert_debug_snapshot!("clamp_pct_table", results);
}
