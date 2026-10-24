//! 集成测试共享辅助：golden 样例装载（单一事实源 = af-dsl 的 testdata，
//! `testdata/sample1-6.yaml`——提案 B §4 六样例）。

/// 装载一个 golden 样例：读文件 + load + 全量校验（零违例断言）。
/// 返回 `(Document, 源文本)`。
pub fn sample(name: &str) -> (af_dsl::Document, String) {
    let path = format!(
        "{}/../af-dsl/testdata/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path:?}：{e}"));
    let doc = af_dsl::load_str(&src).unwrap_or_else(|e| panic!("{name} 装载失败：{e}"));
    let report = af_dsl::validate(&doc);
    assert!(
        report.is_ok(),
        "golden 样例 {name} 应过 af-dsl 全量校验（M1-M4 零违例）：{report}"
    );
    (doc, src)
}

/// 全部 golden 样例文件名。
pub const SAMPLES: [&str; 6] = [
    "sample1.yaml",
    "sample2.yaml",
    "sample3.yaml",
    "sample4.yaml",
    "sample5.yaml",
    "sample6.yaml",
];
