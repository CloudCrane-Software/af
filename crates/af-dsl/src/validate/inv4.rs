//! INV-4 评估器外部化的编译期判定（提案 B §5/§8-5，M3）：
//!
//! 标注 `eval_input: true` 的 store，其读者必须 ⊆ {该 store 的 writer,
//! 绑定 `uses.unit` 的 check 节点}。读者 = 经 `reads[]` 或 `context.sections`
//! 的 from:store 段引用该 store 的全部节点（含体内嵌，按索引全路径标识）。
//! 越界读者每节点一条 → E2331。
//!
//! 评估器判定取 §5 集合字面的严格双条件：kind=check 且 `uses.unit` 在场
//! （gate 属验证面但非评估器；check 缺 unit 亦非评估器）。评估器与 store 的
//! 绑定关系（哪个评估器读哪个参考答案）是运行期组件装配事项（E9003 域），
//! v0.1 对全部 check+unit 节点放行——与 §5 预期错误风格的「evaluator nodes」
//! 复数口径一致（样例 4 的 assess 即此面）。
//!
//! §8-5：标记责任在作者，误标（普通 store 标 eval_input）只会收紧不会放松
//! ——安全性单向，无需反向启发式。
//!
//! 抑制规则（承 M1 偏差⑨「不重复定罪」纪律）：writer 路径悬空已由 refs 层
//! E2001 定罪，该 store 整体跳过（合法读者集合无从建立，双码只产噪声）。

use super::Index;
use crate::error::{ErrorCode, Report};
use crate::model::NodeDecl;
use std::collections::BTreeSet;

/// 是否评估器节点（§5：kind=check 且绑定 uses.unit）。
pub(crate) fn is_evaluator(node: &NodeDecl) -> bool {
    node.kind == "check" && node.uses.as_ref().is_some_and(|u| u.unit.is_some())
}

/// 节点是否读 store s（reads[] 或 from:store 段）。
fn reads_store(node: &NodeDecl, store: &str) -> bool {
    let by_reads = node
        .reads
        .as_ref()
        .is_some_and(|r| r.iter().any(|x| x == store));
    let by_section = node.context.as_ref().is_some_and(|c| {
        c.sections
            .iter()
            .any(|s| s.from == "store" && s.store.as_deref() == Some(store))
    });
    by_reads || by_section
}

pub(crate) fn check(ix: &Index<'_>, report: &mut Report) {
    // 评估器全路径集（对全部 eval_input store 通用，§5「evaluator nodes」口径）。
    let evaluators: BTreeSet<&str> = ix
        .nodes
        .values()
        .filter(|n| is_evaluator(n.node))
        .map(|n| n.path.as_str())
        .collect();

    for (name, store) in &ix.stores {
        if store.eval_input != Some(true) {
            continue;
        }
        // writer 悬空 → refs 层 E2001 已定罪，本层跳过（见模块注释）。
        let Some(writer) = ix.resolve_root_path(&store.writer) else {
            continue;
        };
        for n in ix.nodes.values() {
            if reads_store(n.node, name)
                && n.path != writer.path
                && !evaluators.contains(n.path.as_str())
            {
                report.push(
                    ErrorCode::EvalInputVisible,
                    &n.scope,
                    format!(
                        "store '{}' (eval_input) referenced by node '{}' outside {{writer '{}', evaluator nodes}}（评估输入对候选不可读）",
                        name, n.path, store.writer
                    ),
                );
            }
        }
    }
}
