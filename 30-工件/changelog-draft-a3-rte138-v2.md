# changelog-draft（拟由主代理统一追加到 `计划/变更日志.md`，本批不改原文件）

```markdown
2026-11-05 · A3 RT-E138 键推广返修 v2（feature/a3-rte138-keypromo-v2）：前批 feature/a3-rte138-keypromo 终审不可收卷三项全修——①归因勘正：CH 订单域行来源=投影器 af-otel-projector.py（:408/:437 写 af.work_order_id=首行 stable_id），Rust SpanRow 只写本地状态面 JSON、无消费者入 CH；2026-11-05 14:00 窗 +29 逐单核平为重放/重复行（F1 20 行与 07:44 span_id 重叠 19 + F5c 7 + wo-span-3788989 2，service.name 全是 af-otel-projector/sandbox-harvester），不是「Rust 键推广致 +29」；Rust 侧 order_domain 键入 CH 当前不可直接观测、依赖投影器侧。②撤回「同一真源同一值」：两侧独立取键（Rust=BTreeSet 最小，投影器=首行），容许漂移，判据面以投影器/CH 为准；漂移探针 WARN（attrs.ledger_key_drift + count_key_drift_warns + poc key-drift-probe.py），工厂 otlp-assert.py 零改。③缺 stable_id：注释改写（tracetree 对字段缺席不报 finding）；project_order fail-closed 不产出半键行（修前 publish 1 行 stable_id="" rc=0，修后 rc=1 零行）。夹具 span_produce 13 passed（既有 7 + 双键/计数/旧 schema + 缺键 fail-closed + 漂移 WARN/齐备静默）。例外件：无。遗留：投影器首行空键仍可能落 CH 半键（P2 管 CH，本批不动该链路）。
```
