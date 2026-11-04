# PoC 留证 · 风险#6:graph_version 传递依赖锁(af-workorder)

> 基线:开工时 `origin/product/main`(68be206,施工分支从该点切出)。
> 落点 crate:`crates/af-workorder`(纯 Rust,无新 crate、无新依赖、无机制外设施)。

## 一、改了什么

| 文件 | 行数 | 说明 |
|---|---|---|
| `crates/af-workorder/src/graph_deps.rs` | +444 | **新模块**:graph_version 传递依赖锁。`DepKind`(evaluator/adapter/template)、`DepRef`(kind+name@version 引用)、`DependencyTable`(组件仓声明投影:name→version→直接依赖;重复声明 fail-loud)、`dependency_closure`(不动点闭包,表外引用拒绝)、`graph_deps_digest`(域分离 SHA-256:`af-workorder/graph-deps/v1\0` + 长度前缀材料,对齐 crate 既有 `STABLE_ID_DOMAIN`/`decision_payload` 范式)、`GraphDepsDigest`(64 位小写 hex newtype,`parse` 校验)。内含 5 个模块单测(材料手拼逐段断言、确定性、循环收敛、fail-loud、格式)。 |
| `crates/af-workorder/src/error.rs` | +15 | `GraphDepsError` 四变体(`InvalidReference`/`UnknownDependency`/`DuplicateDeclaration`/`InvalidDigest`),thiserror,集中错误面范式。 |
| `crates/af-workorder/src/lib.rs` | +12/-2 | 模块注册 + re-export;crate 文档能力面补第 7 条。 |
| `crates/af-workorder/tests/graph_deps_contract.rs` | +291 | **跨仓版本契约测试** 9 项(图仓=graph_version+roots,组件仓=DependencyTable;详见下)。 |

**未动**:既有测试断言、golden 快照(`tests/snapshots/*.snap`)、`ci/clippy-baseline.txt`、
crates 集合、根 Cargo.toml、`WorkOrder` 线格式(依赖锁是独立面,不动工单 wire format)。

## 二、为什么这么改(断链现状,已核实)

改前的传递链断在**两头**:

1. **工单面**:`GraphVersion` 是不透明字符串(newtype String),自报什么是什么。
   它不覆盖图依赖方(evaluator/adapter/template)版本——`WorkOrder` 构造面没有
   依赖输入位。同图同串、依赖闭包版本不同 ⇒ 两个"世界"的工单逐字段全等 ⇒ 零检出。
   (probe 实测见 `before/probe-before.txt`:依赖版本进路数 = 0。)
2. **编译面**:全链唯一 digest 是 af-compiler 的 `registry_digest`——**全量声明集合**锁,
   不是图闭包锁:(a) 注册表多版本共存时,图引用取向 `evaluator@1 → @2` 对它不可见
   (表不变 ⇒ digest 不变 ⇒ 漏检);(b) 闭包外组件变化也变 digest(过敏);(c)
   `ComponentDecl`(`components.rs`)**没有依赖字段**——`requires` 被
   `deny_unknown_fields` 拒(实测见 before probe),组件间传递依赖链
   (A 依赖 B、B 依赖 C)在声明面根本无口子。

谁引用谁:图仓(af 侧,graph_version + 直接引用)→ 组件仓(pkg.* 声明)。
断的是:工单面的版本串不含依赖版本;声明面不存在组件间依赖数据 ⇒ "传递闭包"无从算起。

锁语义(改后):`digest = sha256(域前缀 + |len|graph_version + count + 每个闭包成员
|len|kind|len|name|len|version)`(成员按 name,version,kind 字典序)。确定性来自:
材料是明文手拼(无 serde 序列化参与)+ `BTreeSet` 规范序 + 不动点集合;
可复算性:闭包、材料、`sha256_hex` 三件全是公开面,crate 外可逐位重算
(契约测试 `same_graph_same_closure_is_reproducible_outside_the_crate` 断言了这条路径)。

## 三、改前 vs 改后:同一输入的 digest 实测

同一输入:图 `g-3`,依赖链 `pkg.x.evaluator → pkg.x.adapter → pkg.x.template`。

### 改前(基线代码读数,`before/probe-before.txt`)

| 量 | 值 | 结论 |
|---|---|---|
| 工单面"锁" | `GraphVersion("g-3") == GraphVersion("g-3")`,两世界工单全等 | 依赖版本**零进路**,漏检 |
| `registry_digest`(表A:evaluator@1+@2/adapter@1/template@1) | `fe084f42be3f58546a4562569e1ccec68a14b97ca0fa9ee8e174a17a7d3f5ab7` | — |
| `registry_digest`(表B=表A,图取向 @1→@2) | `fe084f42…f5ab7`(**逐位相同**) | **图取向变化漏检** |
| `registry_digest`(表C=表A+无关组件 `pkg.unrelated.other@9`) | `ef4173c71716900ec7b1904813e18fb5f92a509ebb561a524c373dca38e21591`(**变了**) | 全量锁过敏(闭包外也变) |
| 组件声明带 `requires` 字段 | 装载报 `unknown field 'requires'` 被拒 | 传递依赖**无声明面** |

### 改后(`after/contract-after.txt`,9 项契约测试全绿)

| 场景 | digest | 差异来源 |
|---|---|---|
| 闭包 evaluator@1→adapter@1→template@1 | `3b6d59b0ce8728b49603ad6d9ccb2187a53a83bcdb904db6be736ddfccfb5e7e` | — |
| 直接依赖升版(evaluator@2→adapter@2) | `715961f7d6a2b292dbc46eb044723c789346a64d550543ca74c0625aa6cb007b` | 材料中 evaluator/adapter 的版本段变 ⇒ 检出 |
| **传递**:仅 template @1→@2(A/B 版本不动) | `3b6d59b0…5e7e` → `99aa95b4936a9e6a1fea7091ea32b5672b9879b495c43130984e859522a46a04` | C 的版本经两跳传导进 A 的闭包 ⇒ 检出 |
| 多版本共存表,取向 @1 vs @2 | `26d0117cfae550b262610c4d8a2cea8d0f003a51760bfa66c9bea8f66d14d3a0` vs `4d34d37feacf49d794f768aa56871279b754b84d6a6977ccc6efcb77fcfe82a2` | 改前漏检场景(对照 registry_digest 同表同值)⇒ 现在检出 |
| 闭包外加 `pkg.unrelated.report@9` | 与不加**逐位相同** | 闭包锁,不过敏(与 registry_digest 差异位) |
| 可复算 | 材料 `"af-workorder/graph-deps/v1\0|3|g-3|3|7|adapter|13|pkg.x.adapter|1|1|9|evaluator|15|pkg.x.evaluator|1|1|8|template|14|pkg.x.template|1|1"` + `sha256_hex` 在测试内独立重算 = 同值 | — |

### 跨仓契约测试覆盖(`tests/graph_deps_contract.rs`,9 项)

1. `direct_dependency_version_change_is_detected` — 改前漏检场景的检出对照;
2. `transitive_dependency_version_propagates_to_root_digest` — A→B→C,C 变化传导到 A;
   并断言 A/B 成员不变(传导不误伤)+ 闭包 3 成员(跨两跳,不是一层);
3. `multi_version_coexistence_pins_orientation` — 改前 probe 场景改后对照;
4. `outside_closure_change_keeps_digest` — 闭包锁 vs 全量锁差异位;
5. `same_graph_same_closure_is_reproducible_outside_the_crate` — crate 外逐位复算;
6. `graph_version_string_and_kind_are_part_of_the_contract` — 图串/类别都在契约里;
7. `roots_and_declaration_order_are_irrelevant` — 规范序(顺序无关);
8. `empty_closure_still_locks_the_graph_version` — 无依赖图仍钉住 graph_version;
9. `missing_declaration_fails_loud_without_digest` — 组件仓缺声明(直接层+传递层)
   fail-loud,不给"半个闭包"的 digest。

## 四、门读数(施工后,工作树内全量)

| 门 | 命令 | exit | 计数 |
|---|---|---|---|
| 测试 | `cargo test --workspace` | 0 | 153 个测试二进制,**1524 passed / 0 failed**(施工前基线 af-workorder 71 passed / 0 failed) |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | — |
| clippy JSON | `cargo clippy --workspace --all-targets --message-format=json`(强制重检,173 个非缓存 artifact) | 0 | error 级消息 **0 条**(compiler-message 0 条) |
| clippy 门 | `python3 -I ci/clippy_gate.py poc/poc-graph-version-digest/clippy.jsonl` | 0 | `CLIPPY GATE: PASS —— 现有 0 条警告 / 0 签名`(基线 0 签名,未动) |

## 五、边界与如实登记

1. **真实组件仓的依赖声明面仍不存在**:af-compiler 的 `ComponentDecl` 没有
   `requires` 字段(本任务书限定纯 af-workorder 活,未动 af-compiler)。本模块的
   `DependencyTable` 是**投影接口**:上游(编译器/组件仓清单)把声明喂进来才能算锁。
   落地接线(把 `af/packages` 的组件清单扩出依赖面并接进 af-order 提交链)是下一跳,
   需要主代理裁决落点——本 crate 已给出锁原语与确定性/检出语义的全部测试面。
2. **WorkOrder 线格式未加字段**:依赖锁 digest 没有钉进工单/凭证 wire format
   (动了会破既有 golden 快照与既有断言,任务书明令不许)。集成位(如 voucher
   `plan_sha256` 旁加 `graph_deps_digest`)留给后续任务。
3. `clippy.jsonl` 为强制重检输出(627 行,含 173 个非缓存 artifact,0 条
   compiler-message);第一次全缓存跑的 jsonl 与之等价(0 消息),未留双份。
4. 循环依赖按集合不动点收敛(A→B→A 不报错);同名同版本不同 kind 是两个闭包成员
   (kind 是契约的一部分)——这是有意语义,模块文档与单测均有注记。
