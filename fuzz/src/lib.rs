//! fuzz 目标与 `cargo test` 共用的输入构造。
//!
//! 字节布局放在这里，避免 harness 和种子测试各写一套。

pub mod gateway;
pub mod graph;
pub mod sampling;
pub mod workorder;

#[cfg(test)]
mod lock_tests {
    #[test]
    fn f3_cargo_lock_has_no_conflict_markers() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock");
        let text = std::fs::read_to_string(path).expect("fuzz/Cargo.lock");
        let markers = text
            .lines()
            .filter(|line| {
                line.starts_with("<<<<<<<")
                    || line.starts_with(">>>>>>>")
                    || line.starts_with("=======")
            })
            .count();
        assert_eq!(
            markers, 0,
            "F3/607a4b7: Cargo.lock still has conflict markers"
        );
    }

    #[test]
    fn f3_gitignore_has_no_conflict_markers_and_keeps_corpus() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/.gitignore");
        let text = std::fs::read_to_string(path).expect("fuzz/.gitignore");
        assert!(
            !text.contains("<<<<<<<") && !text.contains(">>>>>>>"),
            "gitignore still has conflict markers"
        );
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            assert_ne!(line, "corpus", "corpus seeds must stay in the tree");
        }
    }
}
