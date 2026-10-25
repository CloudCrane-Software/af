# af — Agent 工厂底座（内核 Rust 工作区）

> **本仓库已迁入 agent黑灯工厂 工作区**（../AGENT.md 是所有 agent 的入口契约）：计划与全网状态在 ../计划/，开发前必读。
>
> 当前形态（2026-10-25，第二阶段 G0-G4 已放行、G5 收官中）：**AFB DSL →
> 编译器 → 统一执行面 → 订单门面** 全链在位，配 `af/packages/` 组件与包图
> 登记处；CI 为九门 + advisory。版本明细见 `docs/VERSIONS.md`，AFB 语法定版
> 说明见 `crates/af-dsl/SYNTAX.md`。

## 快速开始

```bash
source ~/.cargo/env        # rustc/cargo 1.99.1（见 BOOTSTRAP.md）
cargo build -j 6 && cargo test -j 6   # 全量构建与测试（当前全绿）
bash ci/run.sh             # 九门验收管道 + step9 FV advisory（产物落 ci/out/）
```

第三方单二进制（temporal/zot/cosign/cue/crane，均已就位于 `bin/`，**不入
git**）的版本与获取方式见 `BOOTSTRAP.md`。

## 当前形态（第二阶段主线）

### 五 crate + graph 执行器（ADR-002）

| crate | 职责 |
|---|---|
| `crates/af-dsl` | AFB 前端：信封 + 七段解析 + 静态校验（L0 不变量/CEL 保守子集/route 归一化/topology_selector）+ 包图清单装载 `registry`。语法定版见 `crates/af-dsl/SYNTAX.md` |
| `crates/af-compiler` | 四阶段编译管线：校验→IR→组件绑定→计划生成（`af-compiler-plan` / `af-compiler-snapshot` CLI；golden 21 份逐字节回归）。ADR-002 规划的 `af-plan` 折入本 crate（`ExecutablePlan`），独立建位仍列开放问题 |
| `crates/af-execline` | 统一执行面：swev（af-bench 迁移）/ graph（图执行器，节点级驱动 planner→coder→reviewer→tester）/ report（报告环 reader→writer）三单元 + planbind 装订 + `af-trace-tree-verify`（订单 trace 四层语义树核对，支持 swev 扁平与 report 环两种 runs 布局） |
| `crates/af-order` | 下单门面 CLI：`submit --unit swev\|graph\|report` 单一入口，直驱 af-engine 9-op 生命周期 + MES 记账 |
| `crates/af-engine` | 订单持久化与驱动（ledger 9-op + MES 四类跃迁 + voucher），既有底座资产 |

其余 17 个 crate 为第一/二阶段底座件（credential/graph/workorder/effect-gateway/
sampling/release-gate/meter/calendar/trace/stop/station/harness/mes/sandbox/
eval/dspy-ir/bench），语义结论在各 crate 的 `RESULT.md`。

### af/packages —— 组件与包图登记处

- **两真包**（图本体 + units/prompts/tools 全实装，G4 门放行）：
  - `sweng/`——软件工程包（`sop_full@1.1` 四节点全链 / `sop_no_qa@1.0` /
    `sop_minimal@1.0` / `fix_swe_task@1.0`），F4D 真单 Closed；
  - `reportkit/`——研究报告包（`paper_report@1.0` 报告环），F5c 真单
    Closed（cite_check+rubric 双 passed）。
- 声明集：`adaskit/ autogen/ swarmkit/` 包图清单 + 根级 `components.json`
  ——组件声明合计 63 条（根 44 + 包级 19），`registry_digest` 入计划对象。
- 规范正本：`af/packages/README.md`。

### AFB 语法（v0.1 + fix1/fix2 + 契约族）

YAML 主干 + CEL 谓词（ADR-001），正本《DSL-语法提案-B》+ 择优裁决
2026-10-24；fix1=E2304 作用域收窄、fix2=topology_selector+registry 注入、
契约族=E 码稳定契约 + golden 21 + 逃逸舱记账。定版说明：
`crates/af-dsl/SYNTAX.md`。

### 形式验证（proofs/，CI advisory）

`proofs/` 收编 FV-1 第一波工件（Kani 32+17 / Verus 29 / Rocq 陈述库），
`ci/run.sh` step9 为非阻塞 advisory 门（Rocq coqc + Kani INV 镜像快检）。
复现命令见 `proofs/README-形式验证.md`。

## CI 九门 + advisory（ci/run.sh）

| step | 门 | 形态 |
|---|---|---|
| 1 | `cargo fmt --check` | 硬门 |
| 2 | clippy + 基线门（只拦新增警告） | 硬门 |
| 3 | `cargo test -j4 --workspace` | 硬门 |
| 4 | CUE recipe vet+export+断言 | 硬门 |
| 5 | parse_scrub × 7 真实日志 | 硬门 |
| 6 | cosign 断网 verify-blob | 硬门 |
| 7 | `cargo deny --offline check` | 硬门 |
| 8 | 中立性门（删包自洽+领域词扫描+golden 登记） | 硬门 |
| 9 | FV advisory（Rocq+Kani 快检） | advisory（FAIL 只 WARN 不红） |

每跑次 SUMMARY 落 `ci/out/` 随分支入档。

## 布局

```
af/
├── crates/               # 22 个内核件（上表五件为第二阶段主线）
├── af/packages/          # 组件与包图登记处（两真包 + 声明集）
├── ci/                   # 九门+advisory 验收管道（run.sh；产物 ci/out/）
├── docs/adr/             # ADR-001..010 裁决集 + VERSIONS.md 版本表
├── poc/                  # 第一阶段 15 项 PoC（各带 RESULT.md / VERIFY*.md）
├── proofs/               # FV-1 第一波形式验证工件（Kani/Verus/Rocq）
├── fuzz/ redteam/        # fuzz target 与红队复验资产
└── RESULT-*.md           # 各批次交付报告（正本随分支入档）
```

## 开发规约

1. **先读对应 crate 的 RESULT.md 再动它**——里面是已验证的语义结论与坑。
2. cargo 一律 `-j 4~6`（本机 16 核 14G 内存）；worktree 构建用
   `CARGO_TARGET_DIR` 独立目录。
3. 零警告纪律；新依赖先过四级漏斗（拍板版 §2.4）。
4. TDD 红绿重构（ADR-007）；语法演进必须 ADR + 全量 golden（C10）。
5. 批次交付以 `RESULT-*.md` 落档；**不设时间估计，不写未来承诺**。
