# F2 全量门「收件箱派单 + 独立 runner」执行面 —— 施工证据（v0.3-T7① F2 批）

> 施工员自述。本目录是**证据面**（新增），不是产品面。
> 工作树：`/home/pan-ding/workspace/af-wt-t7-1-f2`；分支 `feat/t7-1-f2-dispatch-runner`；
> 基线 `origin/product/main` = **56ff52a**（任务书签发时点）。
> **未 commit、未 push、未改 `product/main` 指针、未动在役收件箱/门户/观测面。**
>
> ⚠ **读数时上游已前移**：读数期间 `origin/product/main` 由 56ff52a 前移到
> **f489601**（T5②/T6③ + G1 + T7② 已合主线，非本批所为）。本批自身增量 =
> **4 个新增路径 + 零跟踪文件改动**（§五 有逐条读数）。

---

## 一、现状读准（亲读，非采信任务书）

| 对象 | 读到的形态 | 坐标 |
|---|---|---|
| `ci/run.sh` 十四步管道 | 14 个**槽位**：step1–12 逐个、**step13 共宿 13a/13b/13c**、**step14 共宿 14a–14d**；`step()` 助手登记 `SUMMARY+=("step$id  PASS/FAIL  ${ms}ms  $name")`；失败**不中断**（跑完全部收集诊断）；`OVERALL=0/1`，`exit $OVERALL`；`SUMMARY` 落 `ci/out/SUMMARY.txt`（`tee`） | `ci/run.sh:52-74`（助手）、`:191-199`、`:586-621`（step14 bespoke 块，rc=2 ⇒ `PASS-WITH-WARN` 且**不改** OVERALL）、`:623-631`（汇总+退出码） |
| step14 退出码语义 | bespoke 块（不在 `step()` 助手里）：`machine_batch > "$OUT/step14.log"`；rc=0→PASS、**rc=2→PASS-WITH-WARN（仍绿）**、其它→FAIL 且 `OVERALL=1` | `ci/run.sh:609-621` |
| step9 advisory | 失败只进 SUMMARY 的 `ADVISORY-WARN/ADVISORY-FAIL` 行，**绝不改 OVERALL** | `ci/run.sh:476-489` |
| 硬门槽位 | step1–8、step10–14（step9 不计硬门） | `ci/run.sh` 头注 `:5-29` |
| F2 的两个硬宿主依赖 | **step6 cosign**（`$AF_ROOT/bin/cosign` **不入库**，`.gitignore` 行 2 `/bin`；断网手段 `sudo unshare -n` 优先、回退 `unshare -Urn`）与 **step7 cargo-deny + advisory-db**（`~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`） | `ci/run.sh:289-323`；`docs/CI-BOUNDARY.md:21-22`（跳过档口径） |
| 跳过档口径 | 宿主无 cosign / 无 cargo-deny+advisory-db ⇒ **SKIP（environment_unavailable）**，等价验证 = F2 独立 runner 补跑；**SKIP 不得计绿** | `docs/CI-BOUNDARY.md:21-22` |
| F1/F2 分工 | Actions **只跑** F1 快速门（fmt/clippy/af-dsl+af-compiler 子集，runner 标签 `opch-ci`，`timeout-minutes: 45`）；F2 全量**不进 Actions**，走收件箱派单+独立 runner；F2 结论入账 = integrator 复核制四元组 | `docs/CI-BOUNDARY.md:48-63`；`.gitea/workflows/f1.yml:4-24,39-45` |
| T7② integrator 复核面 | 四元组 `base_sha`/`source_sha`/`tested_tree`/`suite_digest` **由 `emit` 机器自算**；`verdict` 必须**严格等于** `pass`；SUMMARY 词表白名单 = `PASS`/`PASS-WITH-WARN`/`ADVISORY-PASS`/`ADVISORY-WARN`（**不在白名单 ⇒ 一律红**）；**强制回读**门摘要（磁盘↔内嵌 sha256↔内嵌原文 三方比对）；`GATE-BASE-DRIFT` / `COVERAGE-TESTED-TREE` ⇒ rebase 条款 | `ci/integrator_bind_check.py:112-152`（词表/集合）、`:283-320`（摘要判绿）、`:420-564`（复核）、`:650-721`（emit） |
| T7② SOP 的 rebase 条款 | 重落基点 → **重跑 F1**（Actions 在新推送上自动重跑）**且** 重跑 F2（按 `gate_id` 重跑那一门）→ 重写记录件（`-r2`，旧件不许删）→ 再复核；「F1 绿**不替代** F2 记录件」；基点变了但内容没变 ⇒ 可继续但必须写进台账 | `docs/INTEGRATOR-SOP.md:212-252`（§五）、`:156-171`（§三 流程）、`:286-316`（§七 reason_code 回哪一步） |
| 在役派单面（**只读**） | 收件箱落 `af-state/triggers/inbox/<source>/<delivery-id>.json`（七字段信封 + `body_b64` 字节精确原文）；消费腿按注册表 `repo_suites` 映射（**push→F1；pull_request opened/synchronize→F2**）并写 `af-state/triggers/dispatch/<order_id>.json`（`status=dispatch-record-only`）；`order_id` 确定性 = `sha256(repo|suite|subject|recipe_digest)[:16]` | `poc/poc-super-agent/contracts/af.trigger.v1.md:9-40`；`poc/poc-super-agent/registry/trigger-registry.v1.json`（`repo_suites`）；`tools/event-router.py:2503-2531,2770-2841`（在役面，只读） |
| **缺口（本批要补的）** | 派单记录落盘后**无人领跑**（`dispatch/` 目录现役只有 2 个 F1 件，无 F2）；无租约、无退避、无 runner、无结论入账动作 | `af-state/triggers/dispatch/`（实测 2 件，均 F1） |

**本批交付物**：`ci/f2_dispatch.py`（派单侧）、`ci/f2_runner.py`（runner 侧）、
`docs/F2-DISPATCH-RUNNER.md`（契约正文）。

> **读件出处（诚实登记）**：T7② 的 `ci/integrator_bind_check.py` 与
> `docs/INTEGRATOR-SOP.md` **不在本分支基线 56ff52a 内**（它们在读数时已合入主线
> `f489601`）⇒ 本节对这两件的描述**读自主线 `f489601` 的实物**
> （共享树 `/home/pan-ding/workspace/agent黑灯工厂/af`，只读）；
> 其余坐标读自本工作树（基线 56ff52a）的实物。
> 演练的候选树也取自主线 `f489601`（共享树的 sparse shared clone）⇒ 复核面在演练中
> **用的是主线那份真复核器**，不是替身。

---

## 二、派单契约（`af.f2.task.v1`）

正文见 `docs/F2-DISPATCH-RUNNER.md` §一；要点：

* **schema**：复用 `af.trigger.v1` 七字段信封（不重造协议）+ 外层
  `delivery_id`/`event_type`/`body`；派单侧只做「已验信封 → F2 任务件」的翻译。
  任务件字段全表见契约文档 §1.1。
* **投递方式**：**不新增投递通道**。输入 = 在役收件箱信封 + 在役派单记录（**只读**）；
  落点 = runner 自有队列目录（`tasks/ locks/ results/ reported.jsonl dead/ logs/ supply/`），
  与在役收件箱**物理分离**。`ci/f2_dispatch.py ingest|list|show`。
* **幂等键**：`idempotency_key = sha256(order_id|source_sha|base_ref|base_sha|recipe_digest)`；
  三道去重（投递幂等 / 业务 dedup_key / 派单幂等键）正交。
  `order_id` 沿用在役消费腿的**确定性公式** ⇒ 与在役派单记录逐字同值，不符即拒建单。
* **认领唯一写者**：只有 `ci/f2_runner.py` 能领单（派单侧只建单）；
  `flock(LOCK_EX|LOCK_NB)` 互斥（进程死自动释放）+ **租约**（默认 TTL 1800s，跑门期间续租）。
* **租约与超时**：租约未过期 ⇒ 第二个 runner 领不走（`ALREADY-CLAIMED`）；过期 ⇒ 可重领。
* **失败重试与退避**：`attempts+=1`；`>=max_attempts`（默认 3）⇒ `failed` + 死信（留证不删）；
  退避 `min(60s×2^(n-1), 3600s)×(1±20% 抖动)`；退避未到本轮不领（不是失败）。
* **在役面零写入**：派单侧对 `af-state/triggers/**` 只读（实测：本批全程只读）。

---

## 三、runner 执行面（含宿主供给契约 + SKIP 口径）

正文见 `docs/F2-DISPATCH-RUNNER.md` §二/§三。可跑序列：

```bash
python3 -I ci/f2_runner.py supply --repo <候选树>          # 宿主供给核（只读）
python3 -I ci/f2_runner.py run --queue <队列> --task-id <id> --repo <候选树> \
    --worktree <门执行目录> --gate-command "bash ci/run.sh" \
    --summary-path <候选树>/ci/out/SUMMARY.txt --record-dir <候选树>/ci/out
python3 -I ci/f2_runner.py handover --record <记录件> --repo <仓> \
    --base-ref origin/product/main --branch <候选分支>
```

`run` 的脚本级顺序：读任务件 → 幂等判定 → 退避判定 → **领单（flock+租约）** →
候选树绑定核（`HEAD==source_sha` 且工作树干净）→ **宿主供给核** → 跑门（门命令原文入账）→
读 SUMMARY 并按复核器同款词表判绿 + 14 槽位覆盖核 → **结论入账（仅绿时）** →
回报（结果件 + `reported.jsonl`）→ 退租约 + 状态机。

**宿主供给契约**（`af.f2.host_supply.v1`）：`bin/cosign`（不入库）/`cargo-deny`（PATH）/
`advisory-db`（`~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`）/`netns`；
每件记 **sha256 前 12 位 + 版本号**（证据零凭据正文）。本机实测：
`cosign` **缺**（`.gitignore` 行 2 `/bin` ⇒ 不入库）、`cargo-deny 0.20.2` **在**、
advisory-db **在**（files=1388, git_head=999ec89d5d3f）、`unshare -Urn` **可用**。

**SKIP 不计绿的三个落实点**：
1. **runner 层**：缺件 ⇒ **不跑门**、**不写记录件**、`verdict=skip-host-supply-missing`；
2. **摘要层**：SUMMARY 出现白名单外状态（含 `SKIP-*`）⇒ 非绿 ⇒ 不写记录件；
3. **复核器层**：`emit` 对判不出绿的摘要**拒写**记录件；`verify` 对非 `pass` **拒绝**。

**★ 不许把全量门塞进 Actions**：交付物中无任何「在 `.gitea/workflows/*.yml` 跑全量门」
的设计；`.gitea/` **零改动**（门四读数）。

---

## 四、结论入账与 integrator 复核衔接

* **四元组来源**：`ci/integrator_bind_check.py emit` **机器自算**
  （`base_sha=merge-base(base_head, source_sha)`；`tested_tree=source_sha^{tree}`；
  `suite_digest`=`ci/`+`.gitea/workflows/` 在该树内的逐件摘要）；runner **只把记录件里
  那四个值读出来**放进结果件（抄工具产物，不手抄）。
  演练实测四元组：`base_sha=f48960116a60…` `source_sha=f48960116a60…`
  `tested_tree=804cc31c5abc…` `suite_digest=a7c06bc18164…`（门集成员 46 件）。
* **记录件与门摘要同面**：记录件落 `<候选树>/ci/out/integrator-record-f2-<task_id>.json`，
  门摘要落 `<候选树>/ci/out/SUMMARY.txt` ⇒ 复核器可强制回读（磁盘↔内嵌 sha↔内嵌原文）。
* **`tested_tree` 不等价的处置（可执行动作）**：`ci/f2_runner.py handover` 只读复算
  实测侧并产出 5 步动作清单（**不执行** rebase/push）：
  ①重落基点 ②**重跑 F1**（推送候选分支 ⇒ Actions 自动重跑 F1；F1 绿是必要非充分条件，
  **不替代** F2 记录件）③**重跑 F2**（`bash ci/run.sh`，按记录件 `gate_id` 重跑那一门）
  ④重写记录件（`-r2`，旧件不许删）⑤再复核。
  内容等价但基点前移 ⇒ 门结论仍描述这棵树，但「基点变了」必须写进合流报告/台账。

---

## 五、门读数与边界

### 5.1 门读数（原始见 `raw/gate-readings.txt`、`raw/drill-e2e.txt`）

| 门 | 命令 | 退出码 | 关键数字 |
|---|---|---|---|
| 一 fmt | `cargo fmt --check --all` | **0** | 5.7s；**未产生 target 目录** |
| 二 build/test/clippy | — | **未跑** | 本批**禁改 `crates/`** ⇒ 不需要 cargo build/test/clippy；且任务书**严禁本机重编译**（14G 内存）⇒ 如实登记未跑 |
| 三 本批专属门（端到端自证） | `bash poc/poc-t7-1-f2-dispatch-runner/drill/run_drill.sh` | 见 §5.2 | 16 个编号步骤（N-A…N-H **8 条负向臂** + **2 条交接漂移臂**） |
| 四 F1 面零改动核 | `git diff --name-only origin/product/main -- .gitea/` | 0 | **空**（vs 56ff52a 亦空） |
| 五 零弱化核 | `git diff --stat 56ff52a` | 0 | **空**（跟踪文件零改动）；`git status --porcelain` = 4 个新增路径；`crates/` 零 diff；crate 数 **22→22**；`Cargo.toml`/`Cargo.lock` 零 diff；`ci/` 既有件零 diff（跟踪面） |

### 5.2 端到端自证逐条读数（隔离目录 `/home/pan-ding/workspace/f2-drill-scratch/run`）

★ **门是桩门**（`drill/stub_gate.sh`）：只演**管道**，不构成任何真实 F2 门结论。

| # | 臂 | 命令（节选） | 退出码 | 读数 |
|---|---|---|---|---|
| S01 | 候选树 | `git clone --shared --no-checkout` + sparse-checkout `ci .gitea` | 0 | HEAD=f48960116a60…；工作树 0 行变更；`ci/integrator_bind_check.py` 在位 |
| S03 | **N-A 真实缺件** | `f2_runner supply --repo <真实 af 工作树>` | **1** | 缺 `cosign`；cargo-deny 0.20.2 / advisory-db files=1388 / netns 可用 |
| S05 | 派单 | `f2_dispatch ingest` | 0 | created=1；order_id=`F2-opchadmin-af-d41f6a92da893388`；idem_key=3b9ffb96c604… |
| S06 | **N-B 幂等** | 同命令再跑 | 0 | created=0 / **duplicate=1**；队列任务数=1 |
| S07 | **正向全链** | `f2_runner run` | **0** | verdict=**pass**；SUMMARY 16 行 / 14 槽全覆盖 / 非绿步 0；记录件已写 |
| S08 | **复核面读这份 SUMMARY** | `integrator_bind_check.py verify` | **0** | 结论 **PASS**（reason_code=OK，12 项全 OK，含「门摘要回读三方一致」「落地树==门跑过的树」「门没被换」） |
| S09 | **N-C 摘要被改** | 同 verify（摘要追加一行） | **1** | 结论 **REJECT / GATE-SUMMARY-DIGEST**（磁盘 sha 381bb6121905… ≠ 内嵌 2e93d9858a0a…） |
| S10 | **N-D 门红** | 桩门 mode=red | 1 | verdict=**fail**（`SUMMARY-NON-GREEN-STEP`）；**记录件不在位** |
| S11 | **N-E SUMMARY 含 SKIP** | 桩门 mode=skip | 1 | verdict=**fail**（`SUMMARY-NON-GREEN-STEP`）；**记录件不在位** ⇒ SKIP 不计绿 |
| S12 | **N-F 宿主缺件** | 撤走 cosign 替身后 run | 1 | verdict=**skip-host-supply-missing**；**门日志 0 个（门未执行）**；**记录件不在位** |
| S13 | **N-G 租约未过期** | run（他人租约有效） | 1 | `领单失败：ALREADY-CLAIMED：租约未过期（runner=other-runner …）` |
| S14 | **N-H 租约已过期** | run（租约置过期） | **0** | 领单成功 ⇒ 跑门 ⇒ verdict=**pass** ⇒ 记录件已写（emit 落 RECORD-WRITTEN） |
| S15 | 交接（基线未动） | `f2_runner handover` | 0 | aligned=true；base_drift=false；merge_tree=804cc31c5abc…（fast-forward）；suite_equivalent=true |
| S15b | 交接（基点前移但内容等价） | handover `--base-ref c962cec` | 0 | base_drift=**true**；tree/suite 等价=**true** ⇒ aligned=true + 「基点变了必须写进台账」注记 |
| S15c | 交接（**内容不等价**） | handover `--base-ref c1f45f9` | 0 | merge_mode=**MERGE-TREE-CONFLICT**；tree/suite 等价=**false** ⇒ aligned=false ⇒ 出 **5 步动作清单**（重落基点→重跑 F1→重跑 F2→重写记录件→再复核） |
| S16 | 收尾 | — | 0 | 队列任务 5 / 结果件 5 / 回报行 5 / 记录件 2（正向臂+租约臂；红/SKIP/缺件臂 0） |

### 5.3 如实登记的边界

1. **真实 F2 全量门（`ci/run.sh` 十四步）本机未跑**，三条理由：
   ① 宿主缺 `bin/cosign`（不入库）⇒ `ci/run.sh` step0 预检 fail-closed（`ci/run.sh:117-121,186-189`）
   必红；② 本批 diff 触及 `ci/` ⇒ step0 的 mutations 演练判 full（`ci/run.sh:167-185`）
   ⇒ 需重编译，任务书**严禁本机重编译**（14G 内存）；③ 本批**禁改 `crates/`** ⇒
   cargo build/test/clippy 三门按任务书**不跑**。
   ⇒ 因此**没有任何**「真实 F2 门绿」的结论；演练里的一切绿都是**桩门**的管道绿。
2. **桩门不构成门结论**：S07/S08 的 PASS 只证明「派单→领单→跑门→SUMMARY→入账→复核」
   这条管道通，**不证明** fmt/clippy/test/cosign/deny 任何门件通过。SUMMARY 首两行自带
   演练横幅（`F2-DISPATCH-E2E-DRILL banner: gate=stub …`）。
3. **演练用 cosign 替身**：正向臂为了让供给核通过，在**演练候选树的** `bin/cosign`
   放了一个只会打印版本号的脚本（`bin/` 在 `.gitignore` 行 2 ⇒ 不入版本面；不参与任何
   验签）。真实宿主须放**核过哈希的真 cosign**（供给契约 §3.1）。
4. **候选树是 sparse shared clone**：演练候选树 = 共享树的 `git clone --shared --no-checkout`
   + sparse-checkout（`ci` `.gitea`）⇒ 对象共享、**不写 af 仓的 worktree 登记**、
   不复制 1.5G；共享树本身**只读**（未 commit、未改指针）。
5. **合成夹具 ≠ 合成 gitea 投递**：演练夹具是**本地文件面**（`drill/make_fixture.py`
   造注册表+信封+派单记录），**不联网、不发 webhook、不碰在役收件箱**。
6. **`origin/product/main` 在读数期间前移**（56ff52a → f489601）：这是上游合流
   （T5②/T6③+G1+T7②），**非本批所为**；本批自身增量 = 4 个新增路径 + 零跟踪改动
   （§5.1 门五有逐条读数）。
7. **本批 4 个交付件在工作树里是未跟踪状态**（未 commit，按任务书要求）⇒ 合流人需
   `git add` 这 4 个路径后再提交。
8. **`ci/` 下**既有件零 diff**；新增件只有 `ci/f2_dispatch.py`、`ci/f2_runner.py`
   （`.py` 扩展名不在 `ci/gate_inputs_check.sh` 的判据类扩展名表 `CRITERIA_EXTS`
   ⇒ 不触发「门输入全集缺口」红；契约正文因此放在 `docs/` 新文件，不落 `ci/` 一级目录）。
9. **`docs/F2-DISPATCH-RUNNER.md` 是 `docs/` 下的**新增独立文档**（零既有节改动）：
   契约正文若追加进 `docs/CI-BOUNDARY.md` 会与上游 §六（G1 批）产生同位追加冲突。
10. **本批不做**：不改 `ci/run.sh`（不新增 step 编号）、不改 `ci/integrator_bind_check.py`、
    不改 `.gitea/**`、不改 `crates/**`、不改 `Cargo.toml`/`Cargo.lock`、不碰在役收件箱/
    门户/观测面（`tools/trigger-inbox.py`、`tools/af-jobs-portal.py`、`tools/event-router.py`
    等只读）、不建常驻守护、不起监听、不用 sudo、不 commit、不 push、不改
    `product/main` 指针。
11. **未建的**：F2 门套件的**真实调度器**（谁在什么时刻调 `f2_runner run`）不在本批——
    本批交付的是**契约 + 执行面（可跑序列）**，调度由宿主侧作业系统/人工触发
    （本批禁新建常驻守护、不起监听）。
12. **门读数时的副作用已复原**：为证明本批新增件不把既有门读红，单独跑了
    `bash ci/gate_inputs_check.sh check`（exit 0，与移开新增件的对照臂同值）、
    `python3 -I ci/xcheck.py`（exit 0，硬项 8/8）、
    `python3 -I ci/step12_check.py --file numbers.json …`（exit 0，keys=18 watch=1 红=0）。
    其中 `ci/xcheck.py` 会改写**跟踪件** `ci/out/xcheck-report.txt`（门自身行为）⇒
    已 `git checkout -- ci/out/xcheck-report.txt` **复原**；复验后工作树只剩本批
    4 个新增路径（`git status --porcelain`）。

---

## 六、指纹

见本目录 `raw/fingerprints.txt`（sha256 前 12 位 + 字节数；**零凭据正文**）。
