# G3 施工证据（第二批）· 任务包组包**覆盖面**门——「组包路径全覆盖核查缺」收口

> 基线 `origin/product/main` = `56ff52a`（工作树 `git rev-parse` 双向核对一致）。
> 分支 `feat/g3-task-manifest`（不 commit、不 push——diff 留主代理）。
> 债引：`30-工件/剩余欠账总表-20261104/剩余欠账总表.md` G3 行「任务包 manifest
> 缺席 = 不核对的未组包破窗（**组包路径全覆盖核查缺**）｜af 内核面｜未开始」。
>
> **与上批的关系**：本目录上一级（`poc/poc-g3-manifest/` 顶层 6 件，de0ff2e
> 已入 product/main）是 G3 第一批——crate 面「manifest 缺席与核对过类型分开 +
> 提交路径 fail-closed」。本批是债表点名的**剩余正身**：组包路径的**全覆盖
> 核查**（判据面），与上批互补、零重叠、零覆盖面改动。任务书原定证据文件
> `poc/poc-g3-manifest/README.md` 已被上批占用（已提交正本，禁覆盖）——本批
> 证据落本子目录 `coverage-gate/`（仍是纯新增）。

## 现状勘定（比债表口径更进一步的盘面事实）

债表口径「任务包 manifest 不存在」在基线上已部分前移：crate 内
`crates/af-compiler/src/package.rs`（E1 缺口②/G3 一批/G6）已有
`package_manifest.json`——组包接线 `construction.rs:244 write_package_manifest`
写入，钉 `task_sha256`/`plan_sha256`/`plan_id`/`compiler_version`。**但三件
事没人管（债的剩余正身）**：

1. **写入点全集无人核对**——全仓有多少生产写入接线（组包内外）没有任何
   清单造册；新起一个组包写入点=静默绕过一切核对（「只覆盖一处=破窗仍在」）；
2. **盘面多产物无人发现**——层1 只钉 task/plan 两件，任务包目录里**多出来**
   的文件（或组包接线新写的第三种产物）零检出；
3. **层1 钉面与新产物的联动无人强制**——组包将来多产一件而层1 不钉它，
   没有「半覆盖」判定。

---

## ① 生产写入点全集清单（57 点，扫描面实测 + 逐点分类）

**扫描面**（磁盘产物创建 API 全集）：`fs::write` / `File::create` /
`fs::create_dir_all` / `OpenOptions::new(` / `fs::rename` / `fs::copy`，
作用于 `crates/*/src/**/*.rs` 生产区。测试区剔除口径：`#[cfg(test)] mod`
（含**文件中部块**与块闭合后**顶层裸 `#[test]` 函数**——`engine.rs:494`
起、`af-bench/main.rs` 实有形态）+ 经 `lib.rs` `#[cfg(test)] mod X;` 挂载的
独立测试文件。追加写（`.write_all`/`.persist`）经其**创建点**覆盖（文件必
先经上列 API 建）；stdin/socket 管道写非盘面 I/O 不入面。机读清单正本可
随时复扫：`python3 -I ci/task_package_manifest_check.py inventory`（本批
读数 `readings/inventory-mytree.out`）。

**任务包组包域（domain=package，2 点——债的核对面）**：

| 写入点 | 产出 | 说明 |
|---|---|---|
| `crates/af-compiler/src/construction.rs::assemble_construction_package` | `plan.json`、`task.json` | 组包一体入口（任务书所指 `:244` 接线本体；`af-order/src/cli.rs::run_submit` 是其**调用方**而非独立写入点） |
| `crates/af-compiler/src/package.rs::write_package_manifest` | `package_manifest.json` | 层1 运行时 manifest 落盘（经 `PACKAGE_MANIFEST_FILE` 常量；门内常量解析还原产物名） |

**其余 55 点 = 非组包域（逐点造册分类——覆盖面依据；域分类供审计，不进核对面）**：

| 域（点数） | 写入点（file::fn） | 产出 |
|---|---|---|
| compiler-diag（1） | af-compiler/diagnostics.rs::write_to | 机读诊断产物（CLI `--diagnostics-out` 动态路径；非任务包成员） |
| bench（10） | af-bench/main.rs::run_swev / run_tb / run_injec；af-bench/swev.rs::capture_diff / verify；af-execline/suite/swev.rs::capture_diff / verify；af-execline/suite/swev_unit.rs::execute；af-execline/suite/tb_unit.rs::execute | runs 目录、results.jsonl、agent.diff、eval.sh、eval.log、attempt 目录 |
| exec-runtime（17） | af-execline：driver.rs::load_or_create_instance / open_with_sink / execute；engine.rs::write_trust_anchor；exec.rs::execute；gate_unit.rs::persist_gate_order_metrics；graph_unit.rs::drive_task_node / drive_check_node / drive_all / commit / persist_declaration_artifacts；judge.rs::invoke；ledger.rs::append；mes.rs::append_event；record.rs::open / open_append | 实例档案、输出/工件目录、信任锚（tmp+rename 原子）、节点工件、envelope 工件、判分镜像、账本、MES 事件、results/records.jsonl 双轨 |
| sweng-evidence（2） | af-execline/pkg/sweng.rs::publish_task_finished / test_unit | work 目录、test.log、test-evidence.json |
| report（3） | af-execline/suite/report_unit.rs::write_snapshot / draft_fn_real_ring / drive_all | 快照、ring 草稿、report.json / report.md / verify.log |
| eval（1） | af-eval/gate.rs::persist | 判分门留档 |
| harness（1） | af-harness/run.rs::open_with_clock | harness.db（RunStore） |
| mes（2） | af-mes/outbox.rs::open / park | 发件箱目录、partial→最终名（rename 原子） |
| order（7） | af-order：adjudicate.rs::persist / append；admit.rs::append_receipt；cli.rs::run_submit；router.rs::append_consumed；router_liveness.rs::write_json_atomic / write_snapshot | 裁决单、意图账、受理回执、订单目录、消费游标、活体快照（tmp+rename 原子） |
| credential（7） | af-order/voucher.rs::issue_or_load / load_or_create_seed；af-credential/root.rs::open_inner / acquire / write_ready / write_atomic / move_staged / rename_aside | 凭证 JSON、ed25519 种子（create_new 0600）、目录锁、ready 标记、staged→正式 rename |
| trace（5） | af-trace：anchor.rs::append_receipt / atomic_write；trace.rs::open_log / open_prepared；af-release-gate/bin/release-gate.rs::append_event | 锚回执 JSONL、锚原子写、trace JSONL、events JSONL |

全集正本 = `ci/baselines/task-package-writepoints.json`（57 条；生成时与扫描面
双向对账 `scanned-not-registered: [] / registered-not-scanned: []`）。

## ② manifest 设计 + 门设计

**两层 manifest**（任务书三字段——产物路径 / 产物身份（sha256）/ 版本代际——的落位）：

| 层 | 件 | 产物路径 | 身份 | 代际 |
|---|---|---|---|---|
| 层1 运行时（与产出同域，**禁改面**，已在 crate） | `<task_dir>/package_manifest.json`（每包一份，组包接线自己产出） | 包内文件名 ×2（task/plan） | task/plan sha256 逐位 | `plan_id` + `compiler_version`（=af-compiler 版本） |
| 层2 判据面（**本批新增正本**） | `ci/baselines/task-package-writepoints.json`（全仓一份） | 每条目 artifacts（组包域=产物文件名逐位） | 表自身 sha256 自钉于门脚本常量 `REGISTRY_SHA256`（`e4bbf444…46a0`） | `format_version: 1`（拒绝式）+ 自钉（换表=换代际，须同批评审重钉） |

**门 = `ci/task_package_manifest_check.py`**（子命令 `check` / `verify-dir` /
`inventory` / `pin`；退出码 0 绿 / 1 红 / 2 用法环境错），五段 fail-closed：

| 段 | 核什么 | 对应任务书红腿 |
|---|---|---|
| ① 自钉对账 | 表缺席/坏形/`format_version`≠1/哈希≠钉 → 红 | （改判据即改门） |
| ② 写入点全集**双向** | 扫描面每个生产写入点必须在册；在册必须仍有真实写入点（陈年/摘除未销项=红） | **「组包路径覆盖不到」⇒ 红（核心腿）** |
| ③ 组包域闭包 | 注册表组包产物 − {manifest 自身} == 层1 实钉面（自 `package.rs` 的 `file_digest(&task_dir.join("X"))` 字面量机析；**机析失效按红**） | 新产物不被层1 钉=半覆盖红 |
| ④ 组包回环（**真实执行**） | provenance ensure → `cargo run -p af-compiler --bin af-compiler-construct` 现场组真任务包（registry=仓内 `af/packages`，git 内夹具）→ 盘面文件集 ↔ 注册表组包产物集**双向逐位** + 层1 逐字段（sha256 逐位 / `compiler_version`==af-compiler 版本 / 键集闭（deny_unknown_fields 语义）/ `plan_id` 非空） | 清单有盘面无 ⇒ 红；盘面有清单无 ⇒ 红；身份不符 ⇒ 红 |
| ⑤ verify-dir 独立口 | 对任意既有任务包目录单跑 ④ 的核对面（独立复用工具；P4 破窗用它证） | 同上 |

**门位选择与理由**：挂 `ci/run.sh` **step0 预检槽**（`gate_inputs_check.sh`
块后新增一个 0.x 块，`>>"$OUT/step0.log"` 追加写）。理由：

1. **同槽先例同构**：本门与 `ci/gate_inputs_check.sh` 同一模式（全集清单 +
   双向对账 + fail-closed + 改判据须同批重钉），该先例即挂 step0 预检、承
   mutations check-registry 同槽——照挂，不占新 step 编号（任务书硬约束；
   新开槽另须联动 numbers.json 三键 / AGENT.md / README 多文书）。
2. **语义归属**：本门红=覆盖面结构性缺陷（清单/写入点/闭包被破坏），与
   「判据正本漂移」同级，早停合理；不属 step1 fmt/clippy 域，也不属
   step13/14 机读一致性域。
3. **xcheck 门数账不动（实测）**：挂载块用 `>>` 追加写——单 `>` 会被
   `STEP_EXEC_REDIR_RE` 数成执行事实、造出幻影 step0 撞 14 槽对账；
   实测 `count_run_sh_steps(ci/run.sh)`：hard=[1..8,10..14] + advisory=9
   = 14 == numbers.json `ci.steps_total`=14，登记/执行双向集合均不变；
   `bash -n ci/run.sh` 过。
4. **干净克隆可跑（实测）**：门只依赖仓内正本（`af/packages` 夹具在 git
   内、crate 源、python3 标准库）+ cargo；`readings/normal.out` = 在
   `git archive HEAD` 干净克隆面 + 冷 target 上的全量 check 首读（exit 0）。

**注册表为何放 `ci/baselines/`**：`gate_inputs_check.sh` ② 全集自动守卫会把
`ci/` 一级目录下未造册的 `.json/.yaml/...` 判红，而本批**禁改**
`gate_inputs_check.sh`（ci/ 零 diff 约束）——`baselines/` 子目录不在其
`-maxdepth 1` 扫描面（`kani-inv-harness.manifest` 先例在焉），且本门**自带
sha256 自钉**补上外部钉缺位（P5 证：单改表=红）。

## ③ 破窗自证读数（四类各一 + 表篡改 + 正常盘面；全部真跑）

复现：`bash poc/poc-g3-manifest/coverage-gate/run_probes.sh`（自建一次性
克隆面 `git archive HEAD` + 本批新件覆盖，布探/跑门/回收全自动，真仓零接触；
下表 = 该脚本 2026-11-04 实跑落盘读数，`readings/*.out`）。

| 破窗 | 造法（探针） | 读数（exit / 点名行） |
|---|---|---|
| **P1 有生产写入点未覆盖**（核心腿） | 克隆面 `af-station/src/lib.rs` 追加未造册新函数（`create_dir_all` + `fs::write`） | `exit=1`「生产写入点未造册（**覆盖不到=破窗**）：crates/af-station/src/lib.rs::g3_probe_unregistered_writer（写 API 行 [1023,1024]）」 |
| **P2 manifest 有条目、盘面无产物** | 注册表组包条目多造 `meta.json` **并同批重钉**（模拟「造了册但层1 不钉/盘面没有」的合规破窗） | `exit=1` 双红：③「组包域闭包不成立：…['meta.json','plan.json','task.json'] ≠ 层1 实钉面 ['plan.json','task.json']——半覆盖破窗」+ ④「清单有条目、盘面无产物：meta.json（组包产物缺席——漏产物破窗）」（回环真组包后盘面确无 meta.json） |
| **P3 盘面有产物、manifest 里没有** | 克隆面组包接线**真多写** `meta.json`（行为级破窗——今天零门能抓的那类） | `exit=1` 双红：②「组包写入点产出未入册产物：…assemble_construction_package 写 ['meta.json']」+ ④「盘面有产物、清单无条目：meta.json（未造册产物——多产物破窗）」 |
| **P4 产物内容与 manifest 身份不符** | 真组包（`af-compiler-construct`，registry=af/packages）→ 未篡改 `verify-dir` 绿 → 篡改 `task.json` → 红 | 绿 `exit=0`「任务包盘面核对一致…双向逐位」→ 红 `exit=1`「产物身份不符：task.json 实得 sha256=c652e458be99… ≠ package_manifest.json 钉 eb4a3cf8a725…——内容被改/钉错，红」 |
| P5 注册表被改而未重钉 | 表删一条目（不动钉） | `exit=1`「注册表漂移：实得 60fb404b… ≠ 钉 e4bbf444…」+ 被删条目的写入点立刻显形「生产写入点未造册：…voucher.rs::issue_or_load」 |
| 正常盘面 | 克隆面全量 check（冷 target） | `exit=0`「…在册 57 点（组包域 2 点、产物 ['package_manifest.json','plan.json','task.json']），扫描面实得 57 点，回环核对绿」 |
| 终读复绿 | 全探针回收后再跑 | `exit=0` 同上 |
| 挂载层传播 | run.sh step0 挂载块原样语义（破窗面上仿真） | `step0 FAIL: 任务包组包覆盖面红（…）` + `exit=1`（`readings/step0-mount-sim-red.out`） |

**新增测试 21 例全绿**（`python3 -I ci/test_task_package_manifest_check.py`，
2026-11-04 实跑）：

- 扫描器 8 例：生产点命中与归属；`#[cfg(test)]` 中部块 / 裸 `#[test]` /
  lib 挂载文件剔除；行注释免疫；常量名产物解析；多行 `fs::write` 字面量
  窗口；原始字符串花括号深度免疫；
- verify-dir 7 例：正例绿 + 缺件 / 多件 / 篡改 / 代际漂移 / 键集漂移 /
  manifest 缺席六红；
- 注册表钉 3 例：真仓自钉绿 / 钉不符红 / 钉未落（PENDING）红；
- 闭包 3 例：真仓静态零红 / 表加未钉产物红 / 未造册点红。

## ④ 门读数与边界

**四门读数**（原始件 `readings/gates-*.out`；均 2026-11-04 实跑）：

1. `cargo fmt --check --all` → **exit 0**（本批零 .rs 改动）。
2. `cargo test --workspace --no-fail-fast`：
   - 基线（干净克隆面 = `git archive origin/product/main` 正本 + 本批 ci/
     新件，cargo 面与基线同构）：**exit 0，1578 passed / 0 failed /
     0 ignored**（158 段全 ok）——`readings/gates-test-baseline-cleanclone.out`；
   - 本工作树（终跑）：**exit 0，1578 passed / 0 failed / 0 ignored**（158
     段全 ok，与基线逐位一致）——`readings/gates-test-mytree.out`；
   - 同构性结构证明：`git diff origin/product/main --stat -- crates/
     Cargo.toml Cargo.lock` 为空（本批 crates/ 零改动）。
3. clippy 全量重 lint（touch 全部 323 个 .rs 强制重编）→
   `cargo clippy --workspace --all-targets --message-format=json` exit 0、
   **error 级 0 条** → `python3 -I ci/clippy_gate.py <jsonl>` →
   **PASS（现有 0 警告 / 0 签名，均在基线内，基线 0 签名）**——
   `readings/gates-clippy.out`。
4. 判据面零弱化：`ci/` 已跟踪件 diff **仅 `ci/run.sh` 挂载块**（+18 行注释
   与 if 块，零既有行改动）；`ci/clippy-baseline.txt` sha256
   `a48639e7…26ba4` 与基线逐位一致；`crates/` 零 diff、零新增 crate
   （workspace `Cargo.toml`/`Cargo.lock` 未改）。挂载位：不新增 step 编号
   （xcheck 实测 14 槽不变）、干净克隆 check 绿。新门只增加检出
   （`gate_inputs_check.sh`/`clippy_gate.py`/既有断言零触碰）。
   另附：本门自身终读（真树全量 check）`readings/gates-check-mytree-final.out`
   exit 0；新增测试 21 例 `readings/gates-unittests.out` OK。

**如实登记的边界**：

1. **扫描面 API 口径**：只认 6 个磁盘产物创建 API；追加写经创建点覆盖；
   stdin/socket 管道写非盘面不入面。将来引入 `tempfile` crate
   （`NamedTempFile::persist`）或 FFI 直接写盘——在扫描面外：扩 API 面=改门
   脚本（同批评审面）；未扩前是新绕过面（诚实登记，非零剩余风险）。
2. **同函数内新增写入行（非组包域）**：覆盖锚在 (file, fn)——非组包域函数
   **体内**新增写 API 行不改变锚集合，不单独点名红（该函数整体仍在册分类）；
   组包域不受此限（产物字面量逐位对账，P3 证）。债的域界=任务包组包面，
   非组包域分类是审计依据而非核对面。
3. **层1 钉面机析形态依赖**：闭包段按 `package.rs`
   `file_digest(&task_dir.join("字面量"))` 形态机析；写法漂移=机析失效按红
   （fail-closed，但要求同批修门适配新形态）。
4. **注册表外部钉缺位**：本表不在 gate-inputs.sha256 外部钉内（本批禁改
   `gate_inputs_check.sh`）；自钉（脚本常量）补位——单改表红（P5 证），
   表+脚本同批改=一条 diff 走评审。若后续批次愿意把它加进 GATE_INPUTS，
   防护升格为外部钉（本批边界内做不得）。
5. **回环构建成本**：check ④ 段跑 `cargo run`（af-compiler 链）。干净克隆
   冷构建实测通过；CI 内与 step2/3 共享 target 缓存净增≈0，但 step0 首跑
   时点提前支付构建时间。
6. **run.sh 全管道未在本会话全量跑**（四门口径按任务书执行）；挂载层语义
   以同槽 if 块仿真证红传播 + xcheck 门数对账 + `bash -n` 验证。
7. **两次环境面事故（自伤/共盘，均已查明并收口，非门缺陷）**：
   a. **并发 cargo 共享 target**：探针脚本首版继承会话预种的
      `CARGO_TARGET_DIR`，与本树并发 `cargo test` 共 target——探针的
      provenance ensure 判脏即清，掀了测试二进制（exit 101、56/158 段）。
      已改为探针强制独占 target（`PROBE_CARGO_TARGET_DIR` 覆写口）。
      运行纪律注记：门的 ensure 语义与 CI 管道串行用法（step0→step3）无
      冲突，但直连复用同一 target 的并发 cargo 会被判脏互清——正是
      provenance 门要防的跨树污染场景。
   b. **/data 共享盘满**（多工作树并发构建塞满，`No space left on device` +
      `ld terminated with signal 7` 假红——任务书预警的形态）：删除本批
      两个一次性 scratch target（22G）后串行重跑，终读数全真。登记：
      /data 是共享构建盘，本批基线/探针/终跑共占用过 ~42G 峰值，收尾已
      清（g3-cleanclone / g3-probe / 探针克隆面均已删，可由 run_probes.sh
      复现）。
8. **P2 探针的语义**：多造条目**同时重钉**（模拟攻击者改表又改门的「合规
   破窗」）——红腿来自闭包/回环的**内容**判定而非钉，证明钉不是唯一防线。
9. 任务书原文指 `crates/af-execline/src/construction.rs:244`，基线实际文件
   为 `crates/af-compiler/src/construction.rs`（施工以实际盘面为准，证据
   ①的清单已按实际盘面全集造册）。

## 本批改动面（全部；不改 crates/ 任何件）

| 件 | 性质 |
|---|---|
| `ci/task_package_manifest_check.py` | 新增：覆盖面门（check / verify-dir / inventory / pin） |
| `ci/baselines/task-package-writepoints.json` | 新增：层2 注册表正本（57 点，自钉 `e4bbf444…`） |
| `ci/test_task_package_manifest_check.py` | 新增：21 例测试 |
| `ci/run.sh` | 挂载：step0 新增 0.x 块（+18 行，零既有行改动） |
| `poc/poc-g3-manifest/coverage-gate/`（本目录） | 新增：证据（README + run_probes.sh + readings/） |
