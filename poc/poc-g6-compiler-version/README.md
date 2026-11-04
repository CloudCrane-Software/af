# PoC · G6：compiler_version 从「只溯源」到「被核对」

- 工作树：`wt/g6-compiler-version`（独立工作树；未碰 `~/af-dev/af` 共享树）
- 基线：`b8fc5ac`（开工时 origin/product/main；af-compiler 0.1.0）
- 分支：`feature/g6-compiler-version-verify`（提交号见文末）

## 1. 欠账核清（详见 consumer-points.md）

产出面两处（`crates/af-compiler/src/package.rs:48/:127` 任务包 manifest、
`crates/af-compiler/src/plan.rs:77/:459` 计划 compiled_from），值恒为
af-compiler 自身 `env!("CARGO_PKG_VERSION")`；生产代码全仓**无一处**核对它。
两道消费真门：

- manifest 面：`verify_package_manifest`（package.rs）← af-order 提交门
  `preflight_package_manifest`（cli.rs:1063，正式 + 干跑两路同点）
- 计划面：`planbind::bind`（af-execline planbind.rs，执行面消费唯一正本·裁决9）
  ← af-order 干跑装订预检（cli.rs:583）/ 图执行器（graph_unit.rs:926）/
  报告单元（report_unit.rs:1631）

## 2. 改前探针（probe-before-script.rs / probe-before-output.txt）

真门 = `af_order::cli::run(submit --dry-run true)`（与生产 main 同一入口，零替身）：

| 场景 | 构造 | 改前结果 |
|---|---|---|
| A | manifest.compiler_version="0.0.1-rogue"（摘要不动） | **照过**；留证面回显 rogue 值 |
| B | 计划 compiled_from.compiler_version="0.0.1-rogue" + 重组 manifest（摘要自洽） | **照过**（planbind 四核对全绿） |
| C | manifest 缺 compiler_version 键 | 拒（OR1001 坏形 missing field） |
| D | 计划缺 compiler_version 键（摘要自洽） | 拒（装订 decode 拒 missing field） |

A/B = 破窗实证：拿旧编译器/别家编译器产的任务包照过消费门。

## 3. 核对判据来源与理由

**判据 = `af_compiler::COMPILER_VERSION`**（新增导出常量，lib.rs——af-compiler
编译期 `env!("CARGO_PKG_VERSION")` 钉值），即**链接进当前消费二进制的
af-compiler 版本**。理由：

1. 消费侧（af-order 门面 / af-execline 装订器）path 依赖 af-compiler，构建时
   链接的正是这个版本——用它核对即 G6 的语义：「这份产物跟现在要消费它的
   编译器是不是同一个」。
2. 与 registry_digest 先例同构：编译期值随产物携带，消费侧以本地正本对照
   （registry 用本地装载重算 digest；compiler_version 用链接版本的编译期常量）。
3. 不用仓内钉值（与实际链接的 crate 漂移成第二真相）；不用消费 crate 自身
   `env!`（那是 af-order/af-execline 的版本，语义错位）；产物携带值是被核对
   对象本身，不能自证。

## 4. 落地改动（三件，全在既有 crate）

| 文件 | 改动 |
|---|---|
| `crates/af-compiler/src/lib.rs` | 新增 `pub const COMPILER_VERSION`（判据正本） |
| `crates/af-compiler/src/package.rs` | ① 新错误变体 `PackageError::CompilerVersionMismatch{recorded,actual}`（Display 带「重新组包」行动指令）；② `verify_package_manifest` 在坏形/摘要核对**之后**加版本核对（既有错误优先级不变：坏形 → 内容改 → 编译器不符——坏形用例带 compiler_version:"0" 仍先报 format_version=99，既有断言不动）；③ 写入点改用 `crate::COMPILER_VERSION`（值不变，单一真相） |
| `crates/af-compiler/src/plan.rs` | 写入点改用 `crate::COMPILER_VERSION`（值不变）+ 字段注释转「核对面」 |
| `crates/af-execline/src/planbind.rs` | `bind()` ③一致性块加 compiler_version 核对（registry_digest 之后、graph_version 之前），不符 → EX4005 `PlanBindMismatch` 拒（错误分类不扩，文档面登记新核对面） |
| `crates/af-execline/src/error.rs` | PlanBindMismatch 文档注释补 compiler_version 面（纯注释） |

不许静默通过：不符即拒，无降级路径。既有判据零放松（format_version/内容
sha256/registry_digest/graph_version 等原样保留，测试全绿佐证）。

## 5. 测试（18 条新断言，既有断言一条未动）

| 文件 | 新增 | 覆盖 |
|---|---|---|
| `crates/af-compiler/tests/package_manifest_test.rs`（追加） | 4 | 不符拒（专用变体+文案）/ 内容改优先级不变 / 缺键坏形拒 / 相符过 |
| `crates/af-execline/tests/planbind_compiler_version_test.rs`（新） | 3 | 相符过 / 不符 EX4005 拒（双点名+行动指令）/ 缺键 decode 拒 |
| `crates/af-order/tests/compiler_version_gate_test.rs`（新，即改后探针） | 5 | 真门 E2E：相符过（留证回显当前版本）/ A manifest 不符拒 / B 计划不符拒（摘要自洽隔离计划面）/ C、D 缺键拒；**四条拒路全断言状态树零写入**（residue=0 且 state 目录不创建） |

**缺字段口径（如实登记）**：manifest 与计划两面的 `compiler_version` 均为
schema 必填键（`deny_unknown_fields` 拒绝式 decode）——缺字段 = **坏形整份拒**
（OR1001/EX4005 链），不是「显式未核对」，更不静默。此口径改前已成立
（探针 C/D），本批钉死防回退、未放松未改变。

## 6. 改后行为对照（probe-after-output.txt）

| 场景 | 改前 | 改后 |
|---|---|---|
| A manifest rogue | 照过（回显 rogue） | **拒** OR1001 `compiler_version 不符：留档 0.0.1-rogue ≠ 当前构建 af-compiler 0.1.0`+「重新组包」，state 零写入 |
| B 计划 rogue（摘要自洽） | 照过 | **拒** OR1001 装订预检 EX4005 同文案，state 零写入 |
| C/D 缺字段 | 坏形拒 | 坏形拒（口径不变，新断言钉死） |
| 相符（当前构建组包） | 过 | 过（对照组，全绿） |

## 7. 门读数（命令原文可复跑）

| 门 | 结果 |
|---|---|
| `cargo fmt --check --all` | exit 0 |
| `cargo test --workspace` | exit 0；**1552 passed / 0 failed**（gate-cargo-test-workspace.log） |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo clippy --workspace --all-targets --message-format=json` → error 级 | **0 条** → `python3 -I ci/clippy_gate.py`：**PASS（0 警告/0 签名，基线 0 签名内）**；ci/clippy-baseline.txt 一字节未动 |

## 8. 边界与如实登记

- golden 产物（ci/out/g4/*.plan.json 等）携带 compiler_version="0.1.0"——
  产出值与判据同源同值，golden 未动、不受影响。
- af-order `read_plan_graph_version` 是原始 JSON 单键读取（非校验门），不加
  版本核对（那不是它的职责面；真门在 preflight/planbind）。
- 版本语义是**相等核对**（同一构建链接的 af-compiler），不做 semver 兼容
  放行——0.1.0 词表期版本即身份，放宽到「同主版本即过」属放松判据，不为。
- 手工拼 manifest 且伪造当前版本号 + 同时偷改 task/plan 内容的场景由内容
  sha256 核对面守（伪造版本号绕不过摘要）；伪造版本号 + 重组包 = 等价于
  用当前编译器重组包，非本缺口面。

## 文件清单

- probe-before-script.rs / probe-before-output.txt —— 改前探针（A/B 照过破窗实证）
- probe-after-script.rs / probe-after-output.txt —— 改后对照（= compiler_version_gate_test.rs）
- consumer-points.md —— 落点 + 逐消费点清单 + 改前读数
- gate-cargo-test-workspace.log / gate-clippy-Dwarnings.log / gate-clippy.jsonl / gate-clippy-jsonl.stderr —— 门读数
