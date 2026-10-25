# AFB 语法定版说明（v0.1 + fix1/fix2 + 契约族）

> 2026-10-25 · G5 文档版本化批。本文是 AFB（Agent Flow Blueprint）语法的
> **定版状态清单**——正本规范是《DSL-语法提案-B》（研究组）+ 择优裁决
> 2026-10-24；本文只回答「现在语法是什么、定在哪里、靠什么钉住」，不新增
> 规范内容、不写未来承诺。

## 一、v0.1 正本（2026-10-24 择优裁决）

- **形态**：YAML 主干 + CEL 谓词（ADR-001；提案-B 全文 `deny_unknown_fields`）。
- **七段**：`version`（信封键，非 1 整份拒绝）/ `meta`（身份+入口+整图 I/O
  契约）/ `types`（跨节点类型词典）/ `stores`（状态槽：单写者+有界保留）/
  `nodes`（kind 四型 + uses + emits/writes/reads + context + 六构造字段，
  loop/map 构造体与点号路径解析在此）/ `edges`（类型化工件边，IR 1:1）/
  `policies`（budgets[] + stop[] + failures[] 三账合一）。LLM/工具/评估器
  的分野在 `uses` 不在 `kind`。
- **实现**：`crates/af-dsl/src/`（envelope/model/validate/cel/route/shape/
  registry），信封 E0001-E0004 拒绝式；语义违例一次收齐（Report 范式）。
- **表达力边界**：L0 不变量 INV-1..4 编译期判定；不可表达语义走**逃逸舱**
  记账（`body.escapes`，零语法代价；15 稿对齐语料 97 条在案）。

## 二、fix1（2026-10-24，E2304 作用域收窄）

- **内容**：E2304（context.sections 引用未覆盖）判定的实现作用域过宽——
  提案 §5:1347 主语仅 LLM 节点，实现曾误扩到全部节点。fix1（commit
  `73ccfb2`，分支 phase2-dsl-fix1）收窄回规范面。
- **证据**：206 例绿、他稿零变化（E2304-作用域复核-20261024.md）。

## 三、fix2（2026-10-24，topology_selector + registry）

- **内容**：`topology_selector` 编译期解析语义落地 + `af-dsl::registry`
  包图清单装载器（`af/packages/*/manifest.json` → `declared_graphs` 注入）。
  失败分类账 #1-4（ADAS/AutoGen/GPTSwarm/MetaGPT 首轮 E2001 拒）终裁
  「缺组件」由此销项。
- **证据**：4/4 草稿注入后重审零违例；golden 基线 17→21（@824c4b2）；
  独立验证（空 registry 反例复现 + 无隐式放行 + 10 例 manifest 拒绝）。
- **manifest 规范正本**：`af/packages/README.md`。

## 四、契约族（钉住语法的四层契约）

1. **E 码稳定契约**（提案-B §6 码表预算）：错误码分域（E0 信封/E1 形状与
   表达式/E2 语义/E3 装订/E4 执行面），预算余量内新增、禁止私占空号。
   在档新增：E1008 `dotted_node_id`（点号=构造体路径分隔符，id 段禁用）、
   E1054 `cel_limit_exceeded`（深>1000 或长>64KiB 前置拒）——均在 error.rs
   文档注记登记依据，正本回填随规范 v0.2 议程。
2. **golden 21 契约**：`crates/af-compiler/testdata/golden/` 21 份 IR 快照
   逐字节回归 + 篡改检测——语法/编译器任何改动零漂移的机器闸（C10：语法
   演进必须 ADR + 全量 golden）。
3. **语义钉死契约**：`crates/af-dsl/testdata/farm-anomalies/` 39 份对抗
   农田样本 + `tests/farm_anomalies_test.rs` 13 个表驱动测试（39/39）——
   含预算 scope 穿体语义（C_AL09，FALSE-POSITIVE-SEMANTIC：合法穿体必过 +
   悬空路径照报 E2207 反例）与文档化边界 14 份 PASS 钉。
4. **组件声明契约**：`uses.*` 引用串 ↔ `af/packages/**/components.json`
   声明（class/name/version/effect_class；63 条，`registry_digest` 入计划
   对象执行面核对 E9004）。

## 五、定版状态

- 上述 v0.1+fix1/fix2 与四层契约族即**当前语法定版**；对齐语料 15/15 走通
  L1-L4 全链（G3-全链闸门-L1L4-v2，2026-10-24）。
- 已登记的开放议程（不属定版面）：提案-B §1.2 `budgets.scope` 记法显式化、
  E1008/E1054 码表正本回填——均入规范 v0.2 议程（编排者并档）。
