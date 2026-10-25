# ANOMALIES.md — af-dsl 验证器异常清单（afv1-farm，2026-10-24）

被测对象：`af-dsl` 工作树 `/home/pan-ding/workspace/af-wt-phase2-dsl`（branch `phase2-g2b`，
commit `e85bfdbec78f141547d5e3c7df032aca76ee7b6b`，工作树干净）。
判定入口：`af_dsl::load_str` + `af_dsl::validate`（`crates/af-dsl/src/lib.rs:32,55`）。
harness：`~/.cache/afv1-farm`（见农场报告 §2）。异常样本只入报告，未修未改未提交执行。

复现命令（`FARM=~/.cache/afv1-farm/target/release/afv1-farm`；
若二进制已清理，先 `cd ~/.cache/afv1-farm/farm && CARGO_TARGET_DIR=~/.cache/afv1-farm/target cargo build --release`）：

```bash
$FARM one  ~/.cache/afv1-farm/anomalies/<样本>.yaml   # 裁定行（子进程隔离）
$FARM detail ~/.cache/afv1-farm/anomalies/<样本>.yaml # 逐条违例/panic 文本
```

---

## A 类：panic / crash（鲁棒性，进程存活面被破坏）

### A-1 `validate()` panic：零节点文档触发 BTreeMap 索引崩溃
- 样本：`B60_nodes_empty.yaml`（`nodes: []`，其余合法最小文档）
- 裁定：`PANIC-CAUGHT layer=validate`（`one` 模式 catch_unwind 兜住；裸调即 panic）
- panic 文本（`detail` 实测）：
  `panicked at crates/af-dsl/src/validate/refs.rs:30:18: no entry found for key`
- 根因：`validate/mod.rs:137-149` 只在遍历节点时 `scopes.entry(label).or_default()` 建
  作用域条目；`nodes: []` 时 `scopes` 永无 `"root"` 键，`refs.rs:30` 的
  `ix.scopes["root"]` 直接 panic。违反 `validate(&Document) -> Report`（`lib.rs:55`）
  的「违例一次收齐」契约——该文档本应报 `meta.entry` 悬空 E2001。
- 稳定复现：`$FARM one anomalies/B60_nodes_empty.yaml` → `PANIC-CAUGHT validate`

### A-2 CEL 深嵌套 → 栈溢出 SIGABRT（catch_unwind 兜不住）
- 样本：`C_CE15_parens_2000.yaml`（括号深 2000）、`C_CE16_parens_100000.yaml`（深 100000）、
  `C_CE19_plus_chain_200k.yaml`（20 万段加链）、`B89_cel_900kb_chain.yaml`（90 万段加链，
  嵌在 `policies.stop[].when`）
- 裁定：四份均 `CRASH-SIGNAL signal-6`（SIGABRT）
- `detail` 实测：`thread 'main' has overflowed its stack / fatal runtime error: stack overflow, aborting`
- 根因：`validate` → `validate/m4.rs:119-123` → `cel.rs:93` `Program::compile` 包了
  `catch_unwind`（模块注记 cel.rs:84-87 自称兜底纪律），但 antlr4rust 递归下降对深嵌套
  表达式是**栈溢出**，非 unwind panic——进程直接 abort。宿主文档字段全可注入
  （stop[].when / route rules[].when / assert / exit_when / selector when）。
- 阈值实测（本机 8MB 主栈）：深 500（`C_CE28`）、深 1000（`C_CE29`）优雅 PASS；
  深 2000 起 abort。1 万重否定（`C_CE17`）、5 万段析取链（`C_CE18`）不崩。
- 对照（正面证据）：YAML 深嵌套走 `serde_yaml` 递归限，优雅 E0001（B74-B77，深 50000
  流式映射 12-18 秒内拒绝，无崩溃）。

---

## B 类：接受-但-按不变量应拒（漏网 / 人工复核队列）

### B-1 路径别名族：点号节点 id 与构造体路径同形 → 索引静默覆盖，四条不变量同时致盲
根因共享：节点 id 无字符集限制（`model.rs:137` `pub id: String`）；语义索引按
「点号拼接路径串」为键（`validate/mod.rs:132-135`），`BTreeMap::insert` 后写覆盖先写且
**无冲突侦测**；重复 id 侦测（E2003）只在同一 scope 内（`mod.rs:137-149`）——
「根图节点 `a.b`」与「`a` 的 loop 体节点 `b`」路径同为 `a.b`、scope 不同、互不报警。

| 样本 | 裁定 | 被致盲的不变量 |
|---|---|---|
| `C_AL01_writer_alias_root_over_body.yaml` | PASS | INV-2：真双写者（loop 体 b 每迭代写 + 根 a.b 写），E2311/E2312 全哑（根双胞胎覆盖索引后「实际写者」=1 且与声明同串，inv2.rs:42-52） |
| `C_AL02_writer_alias_body_wins.yaml` | PASS | 同上（文档序反转、体内节点胜出方向） |
| `C_AL03_map_writer_hidden.yaml` | PASS | E2312：map 体节点写体外 store 完全不可见（覆盖后幸存者无 writes → 实际写者=0，E2312 迭代对象 `ix.nodes.values()` 漏掉真写者） |
| `C_AL06_verifier_selfloop_hidden.yaml` | PASS | INV-3：体内 verifier 自环（v→v）被根双胞胎覆盖 → E2321 哑，生成即验收潜入 |
| `C_AL07_eval_reader_hidden.yaml` | PASS | INV-4：eval_input 越权读者（体内 r 经 from:store 段读）被覆盖 → E2331 哑 |
| `C_AL13_safetypoint_fake_pass.yaml` | PASS | E2322：kind=task 体节点配 kind=check 安全点被放行（安全点路径解析 `resolve_path` 经 `nodes.get(path)` 命中 kind=check 的双胞胎，inv3.rs:57-88 判错对象） |

复现（以 AL03 为例）：`$FARM detail anomalies/C_AL03_map_writer_hidden.yaml` → `VERDICT PASS`。
建议修复方向（仅记录，未实施）：节点 id 字符集收口（禁点号）或索引插入时检测路径冲突。

### B-2 `meta.output.ref` 未登记类型名 → 零违例静默通过
- 样本：`D_209_blast.yaml`（爆破命中）；最小化探针 `M001_output_ref_unregistered_min.yaml`
  （output.ref = "Ghost"，types 表无此名）
- 裁定：`VERDICT PASS`（0 违例）
- 根因：`refs.rs:61` `if !type_resolves(ix, &doc.meta.output.r#ref) { return; }` ——
  ref 不可解析时 E2202 判定**静默短路**；而 E2205 只对 `meta.input`（refs.rs:51-57）、
  stores item/value、consumes、emits 上报，全链路无一处核对 `output.ref` 类型名。
  `meta.output.ref: map`（未登记）同判 PASS（D_209 实测）。

### B-3 `has()` 宏逃过 CEL 白名单（文档-代码分歧）
- 样本：`C_CE04_has_macro.yaml`（`policies.stop[].when: "has(input.q)"`）
- 裁定：PASS。`cel.rs:11` 注记「条件表达式/负号/下标/**宏**不在保守子集内」，
  但 `has()` 是宏展开、`references().functions()` 不登记 → E1052 拦不住。
  对照：三元（E1052，`C_CE01`）、`size()`（E1052）、下标（E1052）均在位。

### B-4 语义目标解析到构造体内（码位缺口，复核存档）
- `C_AL08_failures_route_to_body_node.yaml` — PASS：`policies.failures[].route: "c.b"`
  命中 `ix.nodes` 体内路径键（`refs.rs:597` contains_key），失败重路由可指向体内深层节点。
  **[修复批处置 2026-10-25]** 已修：route/fallback_route 根图限定（`scope_id("root",·)`
  精确 id），体内路径拒 E2204（对齐稿 route 目标全为关键词/根图 id，零回归）。
- `C_AL09_budget_scope_into_body.yaml` — PASS：预算 scope `node:c.b` 解析进体内
  （`refs.rs:569` resolve_root_path），作用域未限定根图。
  **[编排者裁决 2026-10-25：FALSE-POSITIVE-SEMANTIC（语义确认，非缺陷）]** 预算 scope
  允许「构造节点.体内节点」点号路径穿体——G3 放行裁决锁定的过稿语义（9/15 对齐稿
  12 处在用，剔除注释假命中后的逐行解析口径：ADAS 1、AFlow 1、Agensh-DeLM 2、CAMEL 2、
  CrewAI-YAML 1、GPTSwarm 2、MemGPT 1、Reflexion 1、SWE-agent 1；例证 ADAS
  `node:mas_loop.meta_agent`、AFlow `map:search.exec_map` 等），体内节点
  按迭代计费是预算面核心用途；C_AL09 与过稿用法结构同形，农场误报。不改码；语义钉死
  测试在档（farm_anomalies_test::al09_budget_scope_into_body_is_confirmed_semantics，
  含悬空穿体路径照报 E2207 反例）；提案-B §1.2 budgets.scope `<id>` 记法歧义消解记入
  规范 v0.2 议程。

### B-5 阈值类/口径类（复核存档，均按现有代码口径「合理」，列给人工裁决）
- `C_RT06_exit_named_node.yaml` PASS — 节点真的叫 `exit` 时，route 归一化把出边目标
  `exit` 与关键字 `exit` 混叠（`route.rs:88` valid()、`route.rs:108-131` 跳过 exit 分支的
  重复检测），「exit 分支不产边」注释（route.rs:7）在同名节点场景失真。
- `C_IV48` / `C_IV49` PASS — eval_input 经**写者/评估器 emits 端口转发**给图下游
  （inv4.rs:31-42 只认 reads[]/from:store 段直读；端口级数据流静态不可见）。
  「eval_input 间接泄漏」攻击面的实测结论：一跳直读有拦（E2331，`C_IV41/42/47`），
  经端口转发两跳即漏。
- `C_CX20` PASS — 同名入边端口被两条边共用时，单段引用即满足 E2304「恰一」
  （`inv1.rs:53-63` in_ports 先集合去重再计数）。
- `B10_type_schema_str.yaml` PASS — `types[].schema: "x"`（标量）被接受：
  schema 是 `serde_json::Value`，无 object 形状强制（`model.rs:71-74` 注记与实现不符）。
- `B50_retention_zero.yaml` PASS — `retention.max_items: 0` 记「有界」
  （`refs.rs:157` 只查 is_none；Some(0) 值域空洞）。
- `C_CE02/C_CE03` PASS — list/map 字面量过算子白名单（`cel.rs:24-27` 未覆盖的字面量构造）。
- `B23_ops_param_unclosed.yaml` PASS — ops 参数闭包 E2209 未实装（提案边界，已知）。

### B-6 文档化边界（按代码注记为「有意不查」，入档不立案）
- `C_MW03_zero_writer.yaml` / `B69_zero_writer.yaml` PASS — 零写者不报（inv2.rs:6 注记）。
- `C_IV35_indirect_selfverify_documented.yaml` PASS — 间接自验证 v0.1 不查（inv3.rs:6-9，
  正例钉 `indirect_self_verification_not_flagged_v01`）。
- `C_CE10_steps_badfield.yaml` / `C_CE21_div_zero.yaml` PASS — 求值错误不判
  （cel.rs:15-17 注记；字段级词汇核对不在 §1.4 判定面）。

---

## C 类：拒绝-但-直觉应过（误杀 / 人工复核队列）

- `C_AL04_entry_output_asym.yaml` REJECT E2001 — 节点 id 含点号时 `meta.entry` 按
  **精确 id** 命中（refs.rs:30），`meta.output.from` 按**点号分段**解析（refs.rs:37）：
  同一节点可作入口、不可作输出来源，口径分裂误杀全点号 id 图。
- `C_AL12_safetypoint_wrong_target_fk.yaml` REJECT E2322 — 体内 v 真为 kind=check
  （合法安全点），解析命中的是 kind=task 双胞胎 → 误杀（B-1 同根因的另一面）。
- `C_AL14_frombody_shadow_fk.yaml` REJECT E2106 — from_body `b.r` 目标端口真实存在，
  `check_from_body` 经 `nodes.get("mp.b")` 命中无双胞胎端口 → 误杀（refs.rs:299-319）。
- `C_AL10_cel_dotted_id_unreachable.yaml` REJECT E2203 — 点号 id 节点在 CEL 词表登记为
  整名 `"x.y"`（m4.rs:166-180），CEL 表达式按段解析根为 `x` → 该节点永远无法被 CEL
  引用（设计缺口：点号 id 与 CEL 根词法冲突，无迁移指引）。

---

## 覆盖面与未覆盖面（如实登记）

- 已覆盖：load_str+validate 全链（信封 E000x / 形状 E100x / 语义 E2xxx 全码位族）；
  随机爆破 259 份全部收裁定。500/500 两轮全量裁定逐位一致（耗时列除外）——
  `diff <(cut -f1-6 results.tsv | sort) <(cut -f1-6 results_rerun.tsv | sort)` 为空。
- 未覆盖：`af_dsl::registry`（包清单装载，不在 load_str+validate 契约内）；
  `ValidateOptions` 非默认档（context_quota_cap / declared_graphs）仅默认值；
  E900x 部署期码位属 af-compiler；依赖锁文件未逐行核对（cel-interpreter 0.10 /
  serde_yaml 0.9.34 以 crate manifest 声明为准）；栈溢出阈值只在本机 8MB 栈单点标定。
