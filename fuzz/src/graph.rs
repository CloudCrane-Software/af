//! 字节 → 图模板解析，再在模板规模内调用 [`validate`](af_graph::validate)。
//!
//! 环检测是递归 DFS，已验证语义按几十个节点设计。超过上限的输入只完成
//! schema 解析，不进入 `validate`，避免深链耗尽调用栈。

use af_graph::{validate, GraphTemplate, Report};

/// 单次输入的字节上限。serde_json 自己还有递归深度限制。
const MAX_INPUT: usize = 16 * 1024;
/// 单个作用域的节点上限。递归深度约等于节点数。
const MAX_NODES: usize = 48;
const MAX_EDGES: usize = 96;
const MAX_SUBGRAPHS: usize = 8;

/// 跑一条输入。解析失败或超出规模时直接返回。
pub fn fuzz_one(data: &[u8]) {
    let Some(report) = parse_and_validate(data) else {
        std::hint::black_box(data.len());
        return;
    };
    std::hint::black_box(report.is_ok());
    for violation in &report.violations {
        std::hint::black_box(violation.error_code.code());
        std::hint::black_box(violation.rule.name());
    }
}

/// 公共解析入口 + 规模内校验。解析失败或超出规模返回 `None`。
pub fn parse_and_validate(data: &[u8]) -> Option<Report> {
    let template: GraphTemplate = serde_json::from_slice(capped(data)).ok()?;
    if !within_scale(&template) {
        std::hint::black_box(template.nodes.len());
        return None;
    }
    Some(validate(&template))
}

fn capped(data: &[u8]) -> &[u8] {
    let end = data.len().min(MAX_INPUT);
    &data[..end]
}

fn within_scale(template: &GraphTemplate) -> bool {
    template.nodes.len() <= MAX_NODES
        && template.edges.len() <= MAX_EDGES
        && template.subgraphs.len() <= MAX_SUBGRAPHS
        && template
            .subgraphs
            .values()
            .all(|sg| sg.nodes.len() <= MAX_NODES && sg.edges.len() <= MAX_EDGES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_graph::ErrorCode;

    fn read_seed(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/corpus/af_graph_schema_parse/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).expect("corpus seed")
    }

    #[test]
    fn min_legal_seed_parses_and_validates() {
        let report = parse_and_validate(&read_seed("min_legal.json")).expect("parsed");
        assert!(report.is_ok());
    }

    #[test]
    fn orphan_seed_is_unreachable_node() {
        let report = parse_and_validate(&read_seed("orphan.json")).expect("parsed");
        assert!(report
            .violations
            .iter()
            .any(|v| v.error_code == ErrorCode::UnreachableNode));
    }

    #[test]
    fn oversized_template_stops_after_parse() {
        let mut nodes = String::new();
        for i in 0..(MAX_NODES + 1) {
            if i > 0 {
                nodes.push(',');
            }
            nodes.push_str(&format!(r#"{{"id":"n{i}","kind":"task"}}"#));
        }
        let json = format!(
            r#"{{"graph_version":"1","name":"big","entry":"n0","nodes":[{nodes}],"edges":[]}}"#
        );
        assert!(serde_json::from_str::<GraphTemplate>(&json).is_ok());
        assert!(parse_and_validate(json.as_bytes()).is_none());
    }

    #[test]
    fn garbage_does_not_panic() {
        fuzz_one(b"");
        fuzz_one(b"{");
        fuzz_one(b"not-json");
    }
}
