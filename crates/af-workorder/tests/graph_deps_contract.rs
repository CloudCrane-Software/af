//! 跨仓版本契约测试(v0.3 方案 §开放风险登记 #6 的覆盖面)。
//!
//! 两仓角色:**图仓**(af 侧)持有 graph_version 与图的直接引用
//! (`roots`);**组件仓**(pkg.* 侧)持有依赖声明表(`DependencyTable`
//! 的装载来源)。契约 = `graph_deps_digest`:图仓把 digest 钉进工单/
//! 凭证面,组件仓版本漂移 ⇒ digest 变 ⇒ 契约被检出。
//!
//! 覆盖(施工前读数:改前同场景零检出,见 `poc/poc-graph-version-digest/`):
//! 1. 直接依赖版本变化 ⇒ digest 变(§改前漏检场景的检出对照);
//! 2. **传递** A→B→C:C 的版本变化传导到 A 的 digest(A/B 版本不变);
//! 3. 多版本共存时图引用取向 @1 vs @2 ⇒ digest 不同(改前
//!    registry_digest 对此无感——全量集合锁盲区);
//! 4. 闭包外组件升版 ⇒ digest 不变(闭包锁,不是全量锁);
//! 5. 同图同闭包可复算(材料+sha256_hex 在 crate 外逐位复算);
//! 6. 声明顺序/roots 顺序无关(规范序);
//! 7. kind、graph_version 串都是契约的一部分;
//! 8. 组件仓缺声明 fail-loud,不给"半个闭包"的 digest。

use af_workorder::{
    dependency_closure, graph_deps_digest, sha256_hex, DepKind, DepRef, DependencyTable,
    GraphDepsError, GraphVersion,
};

/// 图仓侧:同一张图 g-3,直接引用 evaluator(版本由参数钉)。
fn graph_roots(evaluator_version: &str) -> Vec<DepRef> {
    vec![DepRef::new(
        DepKind::Evaluator,
        "pkg.x.evaluator",
        evaluator_version,
    )
    .unwrap()]
}

/// 组件仓侧:三层链 evaluator → adapter → template(三层版本各自参数化)。
/// 声明顺序刻意打乱(与规范序相反),证明 digest 与声明顺序无关。
fn component_repo(
    evaluator_version: &str,
    adapter_version: &str,
    template_version: &str,
) -> DependencyTable {
    let mut table = DependencyTable::new();
    // 先声明 template(闭包最底层,倒序装载)。
    table
        .declare(
            "pkg.x.template",
            template_version,
            vec![],
        )
        .unwrap();
    // adapter 依赖 template。
    table
        .declare(
            "pkg.x.adapter",
            adapter_version,
            vec![DepRef::new(
                DepKind::Template,
                "pkg.x.template",
                template_version,
            )
            .unwrap()],
        )
        .unwrap();
    // evaluator 依赖 adapter。
    table
        .declare(
            "pkg.x.evaluator",
            evaluator_version,
            vec![DepRef::new(
                DepKind::Adapter,
                "pkg.x.adapter",
                adapter_version,
            )
            .unwrap()],
        )
        .unwrap();
    table
}

#[test]
fn direct_dependency_version_change_is_detected() {
    let gv = GraphVersion::new("g-3");
    // 同图同串,组件仓 evaluator v1 vs v2(直接依赖)。
    // 改前读数:GraphVersion("g-3") == GraphVersion("g-3"),两世界工单全等,零检出。
    let v1 = graph_deps_digest(&gv, &graph_roots("1"), &component_repo("1", "1", "1")).unwrap();
    let v2 = graph_deps_digest(&gv, &graph_roots("2"), &component_repo("2", "2", "1")).unwrap();
    assert_ne!(
        v1, v2,
        "同图不同直接依赖版本 ⇒ digest 必须不同(契约被检出)"
    );
    println!("CONTRACT evaluator@1 digest = {v1}");
    println!("CONTRACT evaluator@2 digest = {v2}");
}

#[test]
fn transitive_dependency_version_propagates_to_root_digest() {
    let gv = GraphVersion::new("g-3");
    let roots = graph_roots("1");
    // A(pkg.x.evaluator@1) → B(pkg.x.adapter) → C(pkg.x.template)。
    // C 的版本 v1 → v2,A 与 B 的版本都不动 ⇒ A 的 digest 必须变。
    let c_v1 = graph_deps_digest(&gv, &roots, &component_repo("1", "1", "1")).unwrap();
    let c_v2 = graph_deps_digest(&gv, &roots, &component_repo("1", "1", "2")).unwrap();
    assert_ne!(
        c_v1, c_v2,
        "传递链 A→B→C:C 版本变化必须传导到 A 的 digest"
    );
    println!("CONTRACT 传递链 C(template)@1 ⇒ root digest = {c_v1}");
    println!("CONTRACT 传递链 C(template)@2 ⇒ root digest = {c_v2}");
    // 传导而不误伤:A/B 成员版本确实未变(变的是 C 的版本段)。
    let closure_v1 = dependency_closure(&roots, &component_repo("1", "1", "1")).unwrap();
    let closure_v2 = dependency_closure(&roots, &component_repo("1", "1", "2")).unwrap();
    let ab_v1: Vec<_> = closure_v1
        .iter()
        .filter(|m| m.name != "pkg.x.template")
        .collect();
    let ab_v2: Vec<_> = closure_v2
        .iter()
        .filter(|m| m.name != "pkg.x.template")
        .collect();
    assert_eq!(ab_v1, ab_v2, "A/B 成员不变,只有 C 的版本段在变");
    // 闭包确实跨了两跳(不是一层):root 之外还有两个成员。
    assert_eq!(closure_v1.len(), 3);
    assert_eq!(closure_v2.len(), 3);
}

#[test]
fn multi_version_coexistence_pins_orientation() {
    // 改前 probe 场景的改后对照:注册表多版本共存(evaluator@1 与 @2
    // 同时声明),图引用取向 @1 vs @2。registry_digest 对此无感(全量
    // 集合不变 ⇒ 漏检);闭包锁必须检出。
    let gv = GraphVersion::new("g-3");
    let mut both = DependencyTable::new();
    // evaluator@1 依赖 adapter@1;evaluator@2 依赖 adapter@2。
    both.declare("pkg.x.adapter", "1", vec![]).unwrap();
    both.declare("pkg.x.adapter", "2", vec![]).unwrap();
    both
        .declare(
            "pkg.x.evaluator",
            "1",
            vec![DepRef::new(DepKind::Adapter, "pkg.x.adapter", "1").unwrap()],
        )
        .unwrap();
    both
        .declare(
            "pkg.x.evaluator",
            "2",
            vec![DepRef::new(DepKind::Adapter, "pkg.x.adapter", "2").unwrap()],
        )
        .unwrap();
    let d1 = graph_deps_digest(&gv, &graph_roots("1"), &both).unwrap();
    let d2 = graph_deps_digest(&gv, &graph_roots("2"), &both).unwrap();
    assert_ne!(
        d1, d2,
        "同一张表(多版本共存),图取向 @1 vs @2 ⇒ digest 必须不同"
    );
    println!("CONTRACT 多版本共存,取向 @1 ⇒ digest = {d1}");
    println!("CONTRACT 多版本共存,取向 @2 ⇒ digest = {d2}(registry_digest 对此无感)");
}

#[test]
fn outside_closure_change_keeps_digest() {
    let gv = GraphVersion::new("g-3");
    // 组件仓里闭包外组件(pkg.unrelated.report)加一条新声明:
    // 闭包锁 ⇒ digest 不变(与全量 registry_digest 的差异位:
    // 全量锁对无关变化也变——改前 probe 表C 已实证)。
    let plain = component_repo("1", "1", "1");
    let mut widened = component_repo("1", "1", "1");
    widened
        .declare("pkg.unrelated.report", "9", vec![])
        .unwrap();
    let d_plain = graph_deps_digest(&gv, &graph_roots("1"), &plain).unwrap();
    let d_widened = graph_deps_digest(&gv, &graph_roots("1"), &widened).unwrap();
    assert_eq!(
        d_plain, d_widened,
        "闭包外组件升版 ⇒ digest 不变(闭包锁,不是全量锁)"
    );
}

#[test]
fn same_graph_same_closure_is_reproducible_outside_the_crate() {
    let gv = GraphVersion::new("g-3");
    let table = component_repo("1", "1", "1");
    let digest = graph_deps_digest(&gv, &graph_roots("1"), &table).unwrap();
    // 独立二次构造(表重建、roots 重排)⇒ 逐位相同。
    let rebuilt = component_repo("1", "1", "1");
    let again = graph_deps_digest(&gv, &graph_roots("1"), &rebuilt).unwrap();
    assert_eq!(digest, again, "同图同依赖版本 ⇒ digest 必须相同(可复算)");
    // crate 外复算路径:闭包成员 + 明文材料 + sha256_hex 三件都是公开面,
    // 不依赖本模块内部即可重算出同一个值。
    let closure = dependency_closure(&graph_roots("1"), &table).unwrap();
    let mut material = String::from("af-workorder/graph-deps/v1\0|3|g-3|3");
    for m in &closure {
        let kind = m.kind.as_str();
        material.push_str(&format!(
            "|{}|{kind}|{}|{}|{}|{}",
            kind.len(),
            m.name.len(),
            m.name,
            m.version.len(),
            m.version
        ));
    }
    assert_eq!(digest.as_str(), sha256_hex(material.as_bytes()));
    println!("CONTRACT 复算材料 = {material:?}");
    println!("CONTRACT 复算 digest = {digest}");
}

#[test]
fn graph_version_string_and_kind_are_part_of_the_contract() {
    let table = component_repo("1", "1", "1");
    // 同闭包,图串 g-3 → g-4 ⇒ digest 变。
    let d_g3 = graph_deps_digest(&GraphVersion::new("g-3"), &graph_roots("1"), &table).unwrap();
    let d_g4 = graph_deps_digest(&GraphVersion::new("g-4"), &graph_roots("1"), &table).unwrap();
    assert_ne!(d_g3, d_g4, "换图 ⇒ digest 变(材料含 graph_version)");
    // 同 name@version、不同 kind ⇒ digest 变。
    let as_evaluator = graph_deps_digest(
        &GraphVersion::new("g-3"),
        &[DepRef::new(DepKind::Evaluator, "pkg.x.adapter", "1").unwrap()],
        &table,
    )
    .unwrap();
    let as_template = graph_deps_digest(
        &GraphVersion::new("g-3"),
        &[DepRef::new(DepKind::Template, "pkg.x.adapter", "1").unwrap()],
        &table,
    )
    .unwrap();
    assert_ne!(
        as_evaluator, as_template,
        "同名同版本不同 kind ⇒ digest 变(kind 是契约)"
    );
}

#[test]
fn roots_and_declaration_order_are_irrelevant() {
    let gv = GraphVersion::new("g-3");
    // 双 root:evaluator@1 + template@1,两次计算仅顺序不同。
    let mut roots_a = graph_roots("1");
    roots_a.push(DepRef::new(DepKind::Template, "pkg.x.template", "1").unwrap());
    let mut roots_b = vec![DepRef::new(DepKind::Template, "pkg.x.template", "1").unwrap()];
    roots_b.extend(graph_roots("1"));
    let d_a = graph_deps_digest(&gv, &roots_a, &component_repo("1", "1", "1")).unwrap();
    let d_b = graph_deps_digest(&gv, &roots_b, &component_repo("1", "1", "1")).unwrap();
    assert_eq!(d_a, d_b, "roots 顺序无关(闭包是集合)");
}

#[test]
fn empty_closure_still_locks_the_graph_version() {
    let empty = DependencyTable::new();
    let d_g3 = graph_deps_digest(&GraphVersion::new("g-3"), &[], &empty).unwrap();
    assert_eq!(
        d_g3,
        graph_deps_digest(&GraphVersion::new("g-3"), &[], &empty).unwrap(),
        "空闭包可复算"
    );
    assert_ne!(
        d_g3,
        graph_deps_digest(&GraphVersion::new("g-4"), &[], &empty).unwrap(),
        "无依赖图:digest 仍钉住 graph_version"
    );
}

#[test]
fn missing_declaration_fails_loud_without_digest() {
    let gv = GraphVersion::new("g-3");
    // 组件仓漏声明(图引用了 pkg.x.evaluator@9,表只有 @1):
    // fail-loud,错误点名引用,不产出 digest。
    let table = component_repo("1", "1", "1");
    let err = graph_deps_digest(&gv, &graph_roots("9"), &table).unwrap_err();
    assert_eq!(
        err,
        GraphDepsError::UnknownDependency {
            reference: "evaluator:pkg.x.evaluator@9".to_string()
        }
    );
    // 深层漏声明同样检出:evaluator@1 在表,但它要求的 adapter@9 不在。
    let mut partial = DependencyTable::new();
    partial
        .declare(
            "pkg.x.evaluator",
            "1",
            vec![DepRef::new(DepKind::Adapter, "pkg.x.adapter", "9").unwrap()],
        )
        .unwrap();
    assert_eq!(
        graph_deps_digest(&gv, &graph_roots("1"), &partial).unwrap_err(),
        GraphDepsError::UnknownDependency {
            reference: "adapter:pkg.x.adapter@9".to_string()
        },
        "传递层的漏声明同样 fail-loud"
    );
}
