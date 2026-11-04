# PoC · RT-E1 xcheck 摘门保牌 + RT-E6 run.sh 探针清单钉面补全（两笔门红面清偿）

- 分支：`fix/rt-e1-rt-e6-gate-integrity`（基线 = `origin/product/main` = `56ff52a57c022983d74d994fd4e1c63e98f45235`，HEAD 与基线同一 commit，未 commit/未 push——diff 与读数留给主代理）
- 施工面：独立工作树 `~/af-dev/af/wt/rt-e1-rt-e6-gate-integrity`（detached 建树后切本批分支，未动共享树分支指针）
- 指纹：`git diff`（本批全部改动）sha256 `a73dc78397087dff…`；
  `ci/xcheck.py` `ea20d5da55f2de8d…`；`ci/gate_inputs_check.sh` `ad8f86859c970c4b…`；
  `ci/baselines/gate-inputs.sha256` `60333bd0796da393…`（**update 机生成，非手改**）；
  新增测试 `ci/test_xcheck_subgate_census.py` `b0f5883d5ee6397f…`、`ci/test_gate_inputs_rte6_pins.py` `3a6c79438a458e8c…`
- 原始读数：本目录 `readings/`（10 件，文件名 `prefix-*`=修前 / `postfix-*`=修后 / `gate*`=门 / `rte6-*`=RT-E6 自证）
- 沙盒与干净克隆：`/data/bench/targets/rt-e1-rt-e6/{sbx,tamper-demo,clean-clone}`（构建盘 targets/ 下，按任务书边界）

---

## ① 两个面的逐件对账表

### RT-E1：`count_run_sh_steps` 数什么、漏什么（台账原文「只数助手行」的当代核验）

**先说结论：台账原文描述的是历史形态；截至本批前夜，该函数已数两类执行事实行
（`step N "…"` 助手调用行 + `> "$OUT/stepN.log"` 执行重定向行，`ci/xcheck.py`
STEP_HELPER_RE / STEP_EXEC_REDIR_RE），对当前 run.sh 实测 14 槽全对账。但按任务书
判据「核它当前是否还会把『步骤少跑』读成绿」逐类核验后，**仍有一类执行事实被折叠
不计——字母缀共宿子门 `step 13a|13b|13c`：`STEP_HELPER_RE` 的 `[a-z]?` 把字母缀
吞掉、三行折叠成槽号 13（`ci/xcheck.py:100`）。于是「少跑 13b/13c 两道门」对门数账
不可见**——这正是台账「只数助手行（摘门保牌）」在当代的残留真身。**

计数口径演化谱系（读 `.bak` 与代码注记得出）：

| 时期 | 数什么 | 漏什么 | 修复落点 |
|---|---|---|---|
| 最早期（台账 RT-E1 记账时点） | 只数 `step N "…"` 助手行 | bespoke 执行重定向（step9/14） | 已修（10-31 批前，xcheck.py:96-98 注记） |
| 2026-10-31 前 | 数 `echo "== stepN"` 登记行 | 删执行留登记=摘门保牌 | 已修（10-31「摘门保牌」批：数执行事实+双向绑定） |
| **本批前夜（残留洞）** | 数助手行+重定向行的**槽号** | **字母缀共宿子门被折叠：`step 13a/13b/13c`→槽 13，子门多寡不可见** | **本批补（census 臂）** |
| 登记面外（登记为边界，不改） | step0 预检硬门（check-registry / gate_inputs check / 全量演练）用 `>>` 追加重定向、无登记行 | 删 step0 门块对任何计数器不可见 | 见 ④ 边界 |

各槽独立锚覆盖表（「摘槽+同批改 numbers.json」一致化收缩攻击下的存活锚）：

| 槽 | 门 | 修前独立锚（numbers.json 之外） | 修前实测 |
|---|---|---|---|
| step1-8 | 基础硬门 | AGENT.md「九门（step1-8 硬门）」区间算术（check_gates） | 在场 ✓ |
| step9 | FV advisory | `advisory==1` 断言 + fv_advisory 探测 | 在场 ✓ |
| step10/11/12 | 三对账门 | accountability-slot-floor（xcheck）+ FLOOR_SLOTS（step12 ⑤） | 在场 ✓（2026-11-03 批） |
| **step13** | **fix-scan/arm-wiring/budget-cast-lint 三机制共宿** | **无——唯一锚 numbers.json `gates.ci_step13_subgates`（同批改账可吸收）** | **缺口（本批补）** |
| step13 内 13a/13b/13c | 子门多寡 | **无——槽位折叠后删 13b/13c 对 xcheck 不可见** | **缺口（本批补）** |
| step14 | 机读一致性批 | subgates.list 名册绑定（名册自身钉于 gate-inputs） | 在场 ✓（RT-E81 批） |

修前沙盒读数（正本零改动，沙盒副本 `/data/bench/targets/rt-e1-rt-e6/sbx`；工作区侧文书以忠实 stub 代位——本机无 `~/workspace/agent黑灯工厂/`，stub 生成法见 `stub-docs/` 与本 README ④-3）：

| 场景 | 变异 | xcheck | step12 | 读数文件 |
|---|---|---|---|---|
| S0 对照 | 无 | exit 0（硬项 8/8 过） | — | `readings/prefix-S0-control.txt` |
| **S1 摘门保牌** | 删 `step 13b`/`step 13c` 执行行（调用+续行），留 13a 与 `echo "== step13"` 登记行 | **exit 0（硬项 8/8 全绿——门数账眼盲）** | — | `readings/prefix-S1-drop13bc.txt` |
| **S2 一致化吸收** | S1 + numbers.json `gates.ci_step13_subgates` 3→1 同批改账 | （绿） | **exit 0（18 键复算全过——唯一锚被吸收）** | `readings/prefix-S2-step12-absorb.txt` |
| **S3b 整槽连摘+全键改账** | 整槽摘 step13（登记+执行一并删）+ steps_total/ci_steps/ci_hard/step13_subgates 全键收缩 | **exit 0（g0-erratum 降级 advisory「批未齐 4/5」照绿）** | 改账+README/VERSIONS「十四门」改「十三门」后亦绿 | `readings/prefix-S3b-slot13-absorbed.txt` |

⇒ 修前判据成立：**删两道硬门（arm-wiring 三臂防线、budget-cast-lint）+ 同批改一笔账
⇒ 全管道绿**——「门看着绿、实际没在管事」，与 c7f48c4「改判据正本未同批重钉」同型
（锚与被守物同在攻击者可写面）。S3 中 step12 单独先红的是 README/VERSIONS
「十四门」watch 行（`readings/prefix-S3-step12.txt`，5 红）——也是同树可吸收面。

### RT-E6：run.sh 被消费判据正本 vs 当前钉面（三层钉逐件对账）

钉面三层：**族钉** `ci/baselines/gate-inputs.sha256`（GATE_INPUTS 12 件）、
**专用钉** `ci/baselines/mutations-registry.sha256`（mutations/registry.yaml）、
**双钉** `ci/baselines/clippy-baseline.sha256`（clippy-baseline.txt，族钉之外的第二钉）。
另有 numbers.json 值锚（step12 逐键复算消费）单列对照——**值锚是「摘 X+同批改账」
可吸收面，不折抵钉**。

| 被消费判据正本 | 消费者（run.sh 管道） | 族钉 | 专用钉/值锚 | 修前判定 |
|---|---|---|---|---|
| ci/neutrality-expected-fail.txt | step8 neutrality_gate.sh:42 + xcheck ③ | ✓ | redlist.count 值锚 | ✓ |
| ci/neutrality-wordlist.txt | step8 neutrality_gate.sh:43 | ✓ | neutrality.wordlist 值锚 | ✓ |
| ci/neutrality-golden-registry.tsv | step8 neutrality_gate.sh:44 | ✓ | — | ✓ |
| ci/scorechain-manifest.txt | step1 k5-path-lint.py:60 | ✓ | — | ✓ |
| ci/clippy-baseline.txt | step2 clippy_gate.py:28 | ✓ | clippy-baseline.sha256 **双钉** | ✓ |
| ci/arm-wiring.yaml | step13b arm_wiring_check.py:41 | ✓ | — | ✓ |
| ci/branches.yaml | step14a branches_check.py | ✓ | — | ✓ |
| ci/criteria.yaml | step14c criteria_check.py | ✓ | — | ✓ |
| ci/fix-scan.yaml | step13a fix_scan.py:47 | ✓ | — | ✓ |
| ci/budget-cast-whitelist.json | step13c budget_cast_lint.py:196 | ✓ | — | ✓ |
| ci/debt-register.md | step13b/13c 债号在册判定 | ✓ | — | ✓ |
| ci/subgates.list | step14 名册（xcheck+step12④） | ✓（RT-E81 批入） | — | ✓ |
| mutations/registry.yaml | step0 check-registry 探针清单 | —（族外） | mutations-registry.sha256 专用钉 ✓ | ✓ |
| **ci/baselines/kani-inv-harness.manifest** | step9 9b（baseline_calc verify-kani + run.sh:445 count 行） | **✗ 漏钉** | 仅 numbers.json 值锚 manifest_sha256（可吸收面） | **漏（本批补钉）** |
| **proofs/rocq-规范库-v0/theorems-manifest.txt** | step9 9a-2（定理名集+陈述哈希对账；baseline_calc verify-rocq） | **✗ 漏钉** | 陈述哈希行在正本文件**内**=自指；fv.rocq_theorems 只锚行数 | **漏（本批补钉）** |

**修前计数：漏钉 2 件（step9 基线清单两件——`ci/baseline_calc.sh` 对称管理的
9a/9b 正本，kani 件与已双钉的 clippy-baseline.txt 同性质却无族钉，rocq 件连哈希锚
都在被守文件里）；外挂 0 件（既有 12 条钉行逐条核到消费者，无虚胖）。**
补钉后（GATE_INPUTS 14 件 + update 重钉）：**双向零差——漏 0 / 外挂 0**
（机检：`ci/test_gate_inputs_rte6_pins.py` 断言 A/B/C，5/5 过）。

摘牌通道示意（补钉前）：改 kani manifest 的 `count 32→31`（证明集缩水）+
numbers.json 值锚同批改 ⇒ step9 9b 与 step12 复算照绿。补钉后：改 manifest
不重钉族钉 ⇒ step0 gate_inputs_check 红（读数见 ③）。

## ② 落码内容（只许变严：计数覆盖面纯增量，无一行既有判定被移出/放宽）

1. **`ci/xcheck.py`（+83/-1；-1 为 docstring 行扩写）——RT-E1 计数覆盖面**：
   - 新增 `STEP_SUBGATE_RE`（`^step\s+(\d+)([a-z])\s+"`）：把此前被 `[a-z]?`
     吞掉的字母缀升为计数对象——**漏数的那类（字母缀子门执行行）纳入计数**；
   - 新增 `count_run_sh_subgates()`：census `{槽号: sorted(字母缀)}`；
   - 新增盘面锚 `CO_HOSTED_SUBGATES = {13: ("a","b","c")}`——与
     `ACCOUNTABILITY_SLOTS` 同保护级（锚在 xcheck.py 盘面、非 numbers.json
     任何键，「摘门+同批改账」吸收不掉；改表=脚本 diff=评审面）；
   - 新增硬项臂 `check_cohosted_subgates()`（`gates-subgate-census`，fail-closed，
     main 直调不进 check_gates 早退分支——承 roster/floor 同款纪律）：缺字母
     （摘门保牌/整槽连摘）、多字母（13d 类未评审扩槽）、未登记共宿槽（step 5a 类）
     一律硬红；
   - `count_run_sh_steps` 四元组契约**零改动**（check_gates/门数账/头注探针/
     floor 消费面零扰动）——既有两正则一行未动，已数的行一行不移出。
2. **`ci/gate_inputs_check.sh`（+6 行，仅 GATE_INPUTS 清单块）——RT-E6 补钉**：
   新增两件（kani/rocq 基线清单，含消费者注记与补钉理由）。清单增删=脚本 diff
   即评审面：**新增 `ci/baselines/kani-inv-harness.manifest`（9b）与
   `proofs/rocq-规范库-v0/theorems-manifest.txt`（9a）两件**，理由见 ① 表
   （前者原值锚可吸收，后者原哈希行自指）。
3. **`ci/baselines/gate-inputs.sha256`（+2 行）**：**由 `bash
   ci/gate_inputs_check.sh update` 机生成重钉**（14 件；脚本头注「update 机生成、
   禁手改」纪律），非手改；update 先过全集守卫后才落钉。
4. **新增测试两件**（`python3 -I ci/test_*.py` 直跑，unittest、临时夹具、不触真树）：
   - `ci/test_xcheck_subgate_census.py`（7 例）：正本对照绿 + 五负例
     （删 13b/13c / 整槽摘 / 13d / 5a / run.sh 缺席 fail-closed）+ census 单元
     （注释、`step() {` 函数定义、登记行、续行不算执行事实）；
   - `ci/test_gate_inputs_rte6_pins.py`（5 例）：双向零差 A/B + 清单↔钉文件一致 C
     + **摘牌自证 D（篡改 kani manifest / theorems-manifest / debt-register 三件
     之一而不重钉 ⇒ check exit 1 且点名该件）** + 未篡改对照 E 不误红。

## ③ 摘牌自证读数（证明门会咬人，不是摆设；未放宽任何判定换绿）

**RT-E1（census 臂）**——同一变异，修前绿 → 修后红：

| 变异 | 修前 | 修后 |
|---|---|---|
| S1 删 step 13b/13c 执行（留登记横幅） | xcheck **exit 0**，硬项 8/8 全绿（`readings/prefix-S1-drop13bc.txt`） | **exit 1**，`[FAIL] gates-subgate-census`：`census={13:['a']}` vs 锚 `{13:['a','b','c']}`，「缺字母缀 ['b','c']——摘门保牌/整槽连摘形态」（`readings/postfix-S1-drop13bc.txt`） |
| S3b 整槽摘 step13 + numbers.json 全键同批改账（吸收登记锚） | xcheck **exit 0**（g0-erratum 降 advisory「批未齐 4/5」照绿；`readings/prefix-S3b-slot13-absorbed.txt`） | **exit 1**，`[FAIL] gates-subgate-census`：「整槽缺席」——其余 7 臂含被吸收的登记锚全绿，**唯 census 臂红**=锚独立性直接读数（`readings/postfix-S3b-slot13-absorbed.txt`） |

修后对照 S0：正本 run.sh xcheck exit 0，硬项 **9/9** 过（新臂在场且绿，
`readings/postfix-S0-control.txt`）——**只咬摘门，不误伤正本**。

**RT-E6（族钉）**——改判据正本不重钉必须红（临时根 `--root` 真跑，真树零改动）：

```
对照（未篡改）:  check exit 0 —— 判据正本钉对账一致: 14 件
篡改（本批新钉 9b 件 kani manifest `count 32→31`，不重钉）:
                 check exit 1 —— 判据正本漂移：ci/baselines/kani-inv-harness.manifest
                 期望 sha256=66cb6342… 实得 dd67716e…——判据被改须走 update 同批评审重钉
还原后:          check exit 0
```
（`readings/rte6-tamper-kani-manifest.txt`；9a 件与 debt-register 件同型自证在
测试 D 臂，subTest×3 全红。）**新钉的钉行 dd67716e… 与 numbers.json 既有值锚
`fv.kani_inv_harness.manifest_sha256` 逐位一致**——两层互证、无换钉夹带。

## ④ 门读数与边界

### 六门读数（全在本工作树实跑；构建盘 `/data/bench/targets/rt-e1-rt-e6`）

| # | 门 | 读数 |
|---|---|---|
| 1 | `cargo fmt --check --all` | **exit 0**（另：step1 同串 `python3 ci/k5-path-lint.py` exit 0，4 件判分链逐位对锚） |
| 2 | `cargo test --workspace --no-fail-fast` | **exit 0**；158 套件，**passed 1578 / failed 0 / ignored 9**（`readings/gate2-cargo-test-full.log`） |
| 3 | `cargo clippy --workspace --all-targets --message-format=json`（touch 全部 .rs 后） | clippy exit 0，**error 级 0 条**、warning 级 0 条（446KB jsonl；冷目标首跑即全量，编译器诊断零条=与空基线一致）；`python3 -I ci/clippy_gate.py <jsonl>` → **PASS**（现有 0 条警告 / 0 签名，基线 0 签名） |
| 4 | `bash ci/gate_inputs_check.sh check` | **exit 0**——判据正本钉对账一致: **14 件**（`readings/gate4-gate-inputs-check.txt`） |
| 5 | 判据面零弱化 | ci/ 改动件 = 本批三件+新增两测试（`git status` 亲证）；`ci/clippy-baseline.txt` sha256 `a48639e7…` 与 HEAD 逐位一致；`crates/` 零 diff 且 22 成员与 HEAD 逐位一致；xcheck.py 删除行仅 1 条 docstring 扩写、判定逻辑**只增不删** |
| 6 | 干净克隆可跑 | `git clone file://…`（56ff52a）+ 重放本批 diff/新增件 → 门 4 exit 0、两测试 7/7 与 5/5 全过、census 臂对正本 PASS——零在役面依赖 |

**基线对照说明（门 2 三元组）**：本批 `git diff` 只触及 `ci/`（.py/.sh/钉数据）与
`poc/`、新增测试——`crates/`、`Cargo.toml`、`Cargo.lock` 零 diff（git status 亲证）；
全仓 grep 亲证仅两处 rust 测试引用 `ci/`（`exec_contract.rs` 读 `ci/out/g4/` 产物件、
`strategy_library_test.rs` 仅注释提及），均不在本批改动面 ⇒ **rust 面三元组
1578/0/9 即基线三元组**（本树对 rust 而言就是基线）。

**变异演练（本批 diff 触及 ci/** ⇒ should-run 判 `full`）**：
`mutation_runner.sh run --mode full` → **13/13 探针 PASS**（注册表 sha
`188e157a…` 未动）——含 MUT-003/004（摘门保牌 xcheck/step12 臂）、MUT-012/013
（整槽连摘一致化收缩双臂）对新 census 臂共存性的回归。

### 如实登记的边界（未擅自越权的面）

1. **step0 预检硬门块不在门数账**：`>>"$OUT/step0.log"` 追加重定向形态、无
   `echo "== step0"` 登记行——check-registry / gate_inputs check / 全量演练
   三块若被整块删除，对任何计数器不可见（受 git 评审面与 mutations 注册表钉
   间接约束）。补此面需给 step0 造登记行=改 `ci/run.sh`（本批禁改件）——
   **留主代理裁定**。
2. **根台账三件（numbers.json / deliverables.yaml / doc-claims.txt）未入族钉**：
   皆 run.sh 被消费的登记表/承诺正本，但各有活复算/语义检查门（step12 逐键复算、
   step10/11 逐条核），且入族会把「每次合法改账」与「重钉」强耦合。其可吸收面
   由 floor/名册/census 三独立锚兜底。**族扩到仓根=族界变更，留主代理裁定**。
3. **本机无工作区侧文书**（`~/workspace/agent黑灯工厂/` 不在外派机）：xcheck 全量
   跑以忠实 stub 代位（AGENT.md 两行主张句 + 由 `ci/neutrality-expected-fail.txt`
   机生成的 §1.3 + 同期观察期四文书，`stub-docs/` 可再生）；**变异演练中 xcheck 族
   探针（MUT-003/005/008/009/010/011/012）在本机是「缺件崩溃 exit 1」而非判据红**，
   其权威判定在主代理机重跑；本批 census 判据的权威读数是 ③ 的 stub-doc 全量跑
   与 `ci/test_xcheck_subgate_census.py`（直调臂、零文书依赖）。
4. **`ci/xcheck_probe.sh` 未跑**（同上：需工作区正本）。其对照臂（正本不红）与
   本批 census 臂的相容性由 S0/测试覆盖。
5. **strategies/index.yaml、af/packages/README.md+components.json**：run.sh 消费的
   静态登记面但无族钉——各有自有门（step14d R0-R4 / xcheck ② 互为对账）与
   值锚（registry.pkgs 等）。与边界 2 同属族界问题，**未擅自扩钉**。
6. **钉文件与门脚本自身无钉**（.sha256 钉不了自己；checker .py/.sh 属代码评审面）；
   CO_HOSTED_SUBGATES 锚同理——攻击者改 xcheck.py 本身即吸收，该面的防护=
   脚本 diff 评审（与 ACCOUNTABILITY_SLOTS/subgates.list 同级既有约定）。
7. **文书陈旧注记**：`ci/run.sh:137` 头注仍写「11 件判据正本」（RT-E81 批加入
   subgates.list 后实为 12，本批后 14）——run.sh 禁改，登记待主代理同批勘误。
8. **census 臂不消费 numbers.json**（独立性设计）：`gates.ci_step13_subgates`
   值锚与 census 锚并存——合法增删共宿子门须同批改 run.sh + numbers.json +
   CO_HOSTED_SUBGATES 三面（与 subgates.list 名册同款多面同批纪律）。
