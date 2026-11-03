//! 陈述字面反例实证（铁律①：陈述与代码现实冲突时——不改陈述、不改源码、
//! 冲突实证入册）。本模块 harness 断言 theorems-v1 定稿陈述的【字面】含义，
//! 预期 CBMC FAILURE + 反例 = 冲突机器证据。默认不编入（feature 门控），
//! 单独运行取证：cargo kani --features conflict-demo。

use crate::mirror_inv1::{check_node_mirror, NodeMirror};
use crate::mirror_inv2::{inv2_writer_check, Inv2Mirror, DANGLING};
use crate::{C_E2303, C_E2311};

/// 冲突 #1（A-03 陈述字面 vs 实装抑制角落）。
///
/// 陈述（theorems-v1 A-03 定稿）：「任何文档若存在 LLM-bound 节点声明 reads，
/// 则编译被拒绝且 Report 含 E2303」。
/// 实装（inv1.rs:104-115）：reads 只对【已声明】store 判——reads 引用的 store
/// 全部未声明时，refs 层 E2001 已定罪，E2303 抑制不产（模块注记 inv1.rs:21-24）。
/// 分歧角落：llm ∧ reads 非空 ∧ 全部未声明 ⇒ 实装无 E2303（文档仍被拒，但
/// 是经 E2001 而非 E2303——「Report 含 E2303」半边不成立）。
///
/// 预期：assert 失败，CBMC 给出具体反例（即本具体配置）——冲突实证。
#[kani::proof]
fn a03_literal_statement_conflict() {
    let mut n = NodeMirror::legal();
    n.n_secs = 0;
    n.llm_bound = true;
    n.reads_nonempty = true; // 声明了 reads
    n.reads_decl_hit = false; // ……但引用的 store 全部未声明
    let out = check_node_mirror(&n);
    // 陈述字面断言（预期 FAILURE）：
    assert!(out & C_E2303 != 0, "A-03 字面陈述: llm 声明 reads 必含 E2303");
}

/// 冲突 #2（A-07 陈述字面 vs 实装零写者角落）。
///
/// 陈述（theorems-v1 A-07 定稿）：「某 store 的写者集合……基数 ≠ 1，或 ≠ 声明
/// 的 writer ⇒ Report 含 E2311」。基数 0 ≠ 1 在字面域内。
/// 实装（inv2.rs:5-6 模块注记「零写者不报」+ :42-52 判定条件不含 len==0）：
/// 零写者不产 E2311（注记：一致性归 E2xxx 后续模块）。
///
/// 预期：assert 失败，反例 = 零写者配置——冲突实证。
#[kani::proof]
fn a07_literal_statement_conflict() {
    let m = Inv2Mirror {
        writes: [0, 0, 0], // 无任何节点写 S0/S1：基数 0
        in_map: [false, false, false],
        declared: [0, DANGLING], // S0 声明 writer=N0（已解析）；S1 悬空（跳过支）
    };
    let out = inv2_writer_check(&m);
    // 陈述字面断言（预期 FAILURE）：S0 写者集合基数 0 ≠ 1 ⇒ E2311。
    assert!(out & C_E2311 != 0, "A-07 字面陈述: 写者基数 0（≠1）必含 E2311");
}
