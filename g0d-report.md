# g0d-report.md —— G0-D 覆盖与枚举三件套（2026-10-28）

> 批次：G0 逃逸加固 CI 机制批 · D 组（覆盖与枚举）。工作树 `~/workspace/af-wt-g0d`（分支 `g0d`，base=phase2@`e6e05a0`）。
> 落地三机制：① `branches.yaml` 分支覆盖夹具（补 SPEC 标「待补夹具」两处 + `branches_check.py` 校验器）；② 枚举跨文书一致性 `ci/xcheck.py`（3/4 组对账，码位组遗留登记）；③ `criteria.yaml` 六要素判据 schema + `criteria_check.py` + `--emit-checklist` 机生成终验清单。
> 判据来源：`计划/研究组/冲刺批-20261027/验收判据.md` G0-D 节 4 条 + `计划/研究组/G0-CI机制批-实施规格-20261027/D/1-branches|2-xcheck|3-criteria` 三份 SPEC 的 §⑤ + `G0-CI批-总对账.md` §二/§六。
> 环境纪律：`CARGO_TARGET_DIR=/home/pan-ding/afbuild-g0d`（本树专用；所有 cargo 调用均带此环境变量）。开发期只跑靶向检查（`cargo test -p af-dsl --test …`、`cargo clippy -p af-dsl --all-targets`、`cargo fmt -p af-dsl -- --check`、`python3 -m py_compile`、`bash -n`），**未跑全量 `ci/run.sh`**（合流阶段统一跑）；step14 段以隔离 harness 亲跑（见 §五）。
> 路径纪律：三脚本缺省路径一律由 `__file__`/`$AF_ROOT` 推导到**本工作树**（root 钉死，红队 P2-2/总对账 §六.0）；`xcheck.py` 工作区侧文书（AGENT.md/方案/四观察期文书）住工作区，默认值为绝对路径且可 `--doc-*` 覆写。

## 〇、结论

- **D1**：SPEC 两处「待补夹具」全部补齐并落库——`INV1-CTX-FROM-DEFAULT-SKIP`（inv1.rs:333 兜底臂）补成对夹具（正例=五词表外 from → 静默跳过+仅 M1 E1004；负例=词表内值由本臂 judged 出 E2301、不被吞），`M4-INLINE-PRIM-SYNTH`（m4.rs:393 内联四臂）补逐臂参数化负例（非布尔三臂裸绑定 → E1053 点名结果型）；`ci/branches.yaml` v1 = 10/10 covered、缺口 0、fixture 条目 25（正 11+负 14）。`branches_check.py` 正本 exit 0、**10 项 mutation 探针全红**（删负例/锚漂移/夹具虚记/scope 缩水/pending 超窗/schema 篡改/coverage 零覆盖/格式不可识别/text-mode 降级/text-mode 锚失配）。
- **D2**：`ci/xcheck.py` 正本实跑 **硬项 4/4 过**（门数/packages/词表/日期）+ advisory 1 红（run.sh 头注「六步」活漂移，SPEC ④ 正例同款）→ exit 2；**码位组按 SPEC ⑧.2/总对账 §六.2 遗留登记**（每次运行显式 `[LEGACY]` 行，不静默缺席）；4 项反例探针（副本上门数/件数/删名单行/改日期）全红，对照臂不误红。G0 批门数对账已按 G0-A 勘误行（AGENT.md:91「九门+G0 五门=14 step」）实现：本树 1/5 新 step → advisory 提示；合流态模拟 5/5+14 → 硬核 PASS，篡改为 13 → 硬红。
- **D3**：`ci/criteria.yaml`（G4×4 回填 + G0×2 + G0-OBS-1 观察窗）+ `criteria_check.py` 正本 exit 0、终验清单机生成 6 条目==6 criteria；**9 项 mutation 探针**（删 no_data_clause/删 numerator/塞 13 条/删 escalation/waiver 缺 ADR/waiver ADR 悬空/反空过条款换说法/text-mode）+ waiver 带可解析 ADR 降 WARN，全部按判据分红/WARN。唯一数值勘误：G4-4 分母 12→13（R8 判据由红队 W3-C38 新增，step8.log 实证 13），漂移登记在 count_rule。
- **run.sh**：汇总区前追加 step14 bespoke 段（退出码 1→FAIL+OVERALL=1；2→SUMMARY 显式 WARN 行不改 OVERALL；异常退出按硬红），隔离亲跑 rc=2/OVERALL=0/**2.9s**（step3 已构建后）；删 covered 负例的 FAIL 路径亲跑 OVERALL=1。
- **diff 只落本组区域**：`git status --porcelain` = tracked 仅 `ci/run.sh`（+53 行，全部在 step14 段）+ 本组 7 个新文件；`ci/out/{branches-report,xcheck-report,final-checklist}.txt` 为 CI 再生产物，未提交。AGENT.md 零改动；零密钥。

## 一、判据逐条证据

判据来源 = `验收判据.md` G0-D 节（4 条）+ 三份 SPEC §⑤。下表「结论」列 PASS 均以本会话亲跑命令与输出为证。

| # | 判据（出处） | 命令 | 输出摘录 | 结论 |
|---|---|---|---|---|
| 1 | branches.yaml 夹具补最小可用集（工单 G0-D.1 / SPEC-1 ⑤.1②） | `CARGO_TARGET_DIR=/home/pan-ding/afbuild-g0d cargo test -p af-dsl --test branch_fixtures_test` | `running 4 tests` / `ctx_from_outside_vocab_skips_silently_and_shape_reports_e1004 ... ok` / `ctx_from_in_vocab_is_judged_not_swallowed_by_fallback ... ok` / `inline_primitive_ports_synthesize_kind_defaults ... ok` / `inline_primitive_non_boolean_synth_is_e1053 ... ok` / `test result: ok. 4 passed; 0 failed` | PASS |
| 2 | 同上：先红后绿实证（TDD 纪律，ADR-007） | 同上（首版） | 首版第 3 条断言写错（合法 template 段期望 `["E2301"]`，实为干净）→ `assertion left == right failed: left: [] right: ["E2301"]`；改为 `assert!(codes(&src_ok).is_empty())` 后 4/4 绿 | PASS（修的是测试断言，非产品行为） |
| 3 | 登记 10 分支、每分支 ≥1 正+≥1 负或 pending+理由（SPEC-1 ⑤.1②） | `python3 -c "import yaml; d=yaml.safe_load(open('ci/branches.yaml')); print(len(d['branches']))"` + branches_check 正跑 | `branches.yaml entries: 10`；`== branches_check: 登记 10 分支；红项 0；WARN 0`（covered 10 / pending 0，两缺口处留 `closure_note` 记闭合路径，不编造） | PASS |
| 4 | 锚三元组 10/10 在场（SPEC-1 ① 规则 4） | 亲跑 python 逐条核对 `file:line:anchor_prefix`（正本 vs 代码行） | `OK INV1-CTX-FROM-TEMPLATE crates/af-dsl/src/validate/inv1.rs:193`、`OK INV1-CTX-FROM-DEFAULT-SKIP …:333`、`OK INV1-QUOTA-POSITIVE …:337`、`OK SHAPE-SERDE-ERR-CLASSIFY shape.rs:43`、`OK LOOP-E2002-MAX-ITER-POSITIVE loop_guard.rs:26`、`OK LOOP-E2004-SAFETY-POINTS-NONEMPTY …:33`、`OK LOOP-E2002-CARRIED-BOUNDED …:41`、`OK M4-OVER-LIST-UNWRAP m4.rs:263`、`OK M4-INLINE-PRIM-SYNTH …:393`、`OK BIND-REF-PARSE-UNVERSIONED bind.rs:314` | PASS |
| 5 | 夹具名在 cargo test 名单、path:line 落 fn 行（SPEC-1 ① 规则 3，C10 镜像） | `python3 ci/branches_check.py --test-list-file <raw list>` 正跑（逐 fixture 三段名解析+段内对账） | `OK … 正1/负1 …`×10；`== 登记 10 分支；红项 0`（25 条目全过）；另亲跑 `cargo test -p af-dsl -p af-compiler -- --list` 逐名 grep：v0 20 名全 OK，新增 4 名（branch_fixtures_test 四条）+1 名经 branches_check 全量对账 | PASS |
| 6 | SPEC-1 ⑤.3 mutation：删任一 covered 负例 → 门红 | 副本删 `LOOP-E2002-MAX-ITER-POSITIVE` 负例后 `python3 ci/branches_check.py --branches <副本>` | `[FAIL] LOOP-E2002-MAX-ITER-POSITIVE: covered 缺夹具（正例 1/1、负例 0/1）——新分支无夹具即红`，exit=1；step14 集成层亲跑（§五表 #12）：`模拟 OVERALL=1 rc=1`、`模拟 SUMMARY: step14 FAIL 352ms` | PASS |
| 7 | SPEC-1 ④ 反例（锚失配即红，草案未实跑、本批补） | 副本把 inv1.rs:193 改 line=194 | `[FAIL] INV1-CTX-FROM-TEMPLATE: 锚失配 crates/af-dsl/src/validate/inv1.rs:194（anchor_prefix='"template" => {' 不在行文本；实际行：'if s.template_ref.as_deref()…'）——行号漂移即红`，exit=1 | PASS |
| 8 | SPEC-1 ① 其余规则探针（scope 缩水/pending 超窗/schema） | 三副本分跑 | `[FAIL] scope glob 零命中（防 scope 缩水）：crates/af-dsl/src/no_such_file.rs`；`[FAIL] pending 超宽限窗（['INV1-CTX-FROM-DEFAULT-SKIP']；grace_until=2026-10-01，今日 2026-10-28）`；`[FAIL] 顶层 schema 字段必须为 branches-v1`——均 exit=1 | PASS |
| 9 | SPEC-1 ⑤.5：py_compile + YAML 解析 | `python3 -m py_compile ci/branches_check.py` + yaml.safe_load | 无输出 exit=0；`branches.yaml entries: 10` | PASS |
| 10 | xcheck 3/4 组对账正本全过（工单 G0-D.1 / SPEC-2 ⑤.1） | `python3 ci/xcheck.py` | `[PASS] gates`（AGENT.md 主张 9 门 step1-8 硬门+step9 advisory；run.sh 实测 8 基础硬门 step[1..8]+1 advisory=9；另见 G0 批新 step[14] 合计 10）/ `[PASS] packages`（37 件、逐包分解全等）/ `[PASS] redlist`（23=23=23、集合差 []）/ `[PASS] obs-dates`（四文书均 ['2026-10-29 01:05']）/ `[PASS] gates-g0-erratum`（advisory：G0 批未齐 1/5）/ `[FAIL] runsh-header`（advisory 红：头注 6 步 vs 实数）/ `[LEGACY] codepoints …`；`== xcheck: 硬项 4/4 过；advisory 1/2 过`，exit=2 | PASS |
| 11 | 码位组按 SPEC ⑧ 遗留登记（工单 G0-D.1） | 同上（每次运行必出） | `[LEGACY] codepoints 码位/债号/ADR 三方对账未实现（SPEC ⑧.2 / 总对账 §六.2：正本 error.rs 码位表摸排工作量未在本批全量落地）——机制覆盖面按 3/4 组对账交卷（门数/packages/词表）；不静默缺席` | PASS（登记即交付口径，未实现部分如实） |
| 12 | SPEC-2 ⑤.2④ mutation 四探针全红+正本零改动（工单 G0-D.2） | `bash ci/xcheck_probe.sh`（副本全在 mktemp 目录，正本零改动） | `PASS 对照臂：正本 xcheck exit=2` / `PASS 探针 gates：exit=1，gates 红（AGENT.md 主张 10 门…）` / `PASS 探针 packages：exit=1（README 主张十包 35 件；实测 37 件）` / `PASS 探针 redlist：exit=1（表体 23 行；expected-fail 实测 22 行）` / `PASS 探针 obs-dates：exit=1` / `== xcheck_probe: 探针红项 4/4（含对照臂共 5/5 项合预期）`，exit=0 | PASS |
| 13 | SPEC-2 ⑤.3：py_compile | `python3 -m py_compile ci/xcheck.py` | 无输出 exit=0 | PASS |
| 14 | G0 批门数对账（合流演进，总对账 §二 口径） | 合流态模拟：`--run-sh` 副本插 step10-13 echo 行 + 正本 AGENT.md（G0-A 勘误行:91 在场） | 5/5 新 step：`[PASS] gates-g0-erratum`（`AGENT.md 勘误行主张九门+G0 五门=14 step；run.sh 实测 G0 批 5/5 个新 step，合计 14 step`）→ 硬项 5/5 过；篡改副本 erratum=13：`[FAIL] gates-g0-erratum: 红：G0 批勘误口径与 run.sh 实数不符（13 vs 14）——合流批门数账失真`，exit=1 | PASS（合流后硬核、失真即红） |
| 15 | criteria.yaml 六要素 schema 正本合规（工单 G0-D.1 / SPEC-3 ⑤.1） | `python3 ci/criteria_check.py` | `[PASS] criteria.yaml schema 校验过（六要素/观察窗要素/软上限全合规）`，exit=0；`python3 -c yaml.safe_load` → `criteria.yaml gates: 2`（G4×4+G0×2+G0-OBS-1，判据全部摘自盘上门终裁/阶段计划原文） | PASS |
| 16 | 终验清单机生成、条目数==criteria 数（SPEC-3 ⑤.3） | `python3 ci/criteria_check.py --emit-checklist ci/out/final-checklist.md` | `== 终验清单机生成：ci/out/final-checklist.md（判据条目 6==criteria 6）`；产物首两行=`# 终验清单（criteria.yaml 机生成——禁止手写…）` + `# 源：…/ci/criteria.yaml；schema 校验过后方可生成；清单条目数==criteria 条目数` | PASS |
| 17 | SPEC-3 ⑤.2④ mutation：删要素/超上限/删 escalation 必红 | 四副本分跑 | `[FAIL] G4-1: 缺六要素字段 no_data_clause（PP2 口径，缺一不得进终验清单）`；`[FAIL] G4-1: 缺六要素字段 numerator…`；`[FAIL] gate G0: 判据 13 条超软上限 12——须运营者裁决（operator_waiver）或拆分 gate`；`[FAIL] 观察窗 G0-OBS-1: 缺 C7/PP5 要素 escalation`——均 exit=1 | PASS |
| 18 | SPEC-3 ⑧.2 裁决点 1（实施批补 waiver_adr 必填校验） | 两副本分跑（13 条+waiver） | 缺 waiver_adr：`[FAIL] gate G0: operator_waiver 缺字段 waiver_adr（豁免须可追溯：ADR 指针+理由+批准人+日期）` exit=1；带在册 ADR（docs/adr/014-escape-threshold-policy.md）：`[WARN] gate G0: 判据 13 条 >12 软上限，operator_waiver 在场降为 WARN` exit=2；ADR 悬空（999-not-exist.md）：`[FAIL] … waiver_adr 指针不可解析…（ADR 指针必须可解析）` exit=1 | PASS |
| 19 | SPEC-3 ⑤.5：文本降级模式可用且如实 WARN | `python3 ci/criteria_check.py --text-mode` | `[WARN] 文本模式：yaml 库缺席，结构完整性为降级校验（逐键在场性，非逐条目精配）——如实标注` + `[PASS-WITH-WARN] schema 过，含 WARN 项` exit=2；反空过条款换说法（「无数据时不计」）副本：`[FAIL] G0-2: no_data_clause 必须含『无数据…未达成』反空过语义` exit=1 | PASS |
| 20 | diff 只落本组区域（工单 G0-D.3） | `git status --porcelain` + `git diff --stat` | tracked 仅 ` M ci/run.sh`；` ci/run.sh \| 53 +++…`（1 file changed, 53 insertions(+)，全部位于 step14 新增段）；untracked=本组 7 新文件（ci/branches.yaml、ci/branches_check.py、ci/xcheck.py、ci/xcheck_probe.sh、ci/criteria.yaml、ci/criteria_check.py、crates/af-dsl/tests/branch_fixtures_test.rs）+ 3 个 CI 再生产物（ci/out/，未提交）。未触碰 step3/9/13 区域、AGENT.md、他人工作树 | PASS |
| 21 | 报告逐判据给证据（工单 G0-D.4） | 本文件 §一 | 21 行判据×（命令+输出摘录+结论） | PASS |

## 二、D1 交付物与设计要点

- **新夹具文件** `crates/af-dsl/tests/branch_fixtures_test.rs`（4 测试，全部亲跑绿）：
  - `ctx_from_outside_vocab_skips_silently_and_shape_reports_e1004`（inv1.rs:333 正例）：`from: recall`（五词表外）+ quota 100 → 恰 `["E1004"]` 且逐字断言 `context.sections[0] 来源 "recall" 不在闭环词表 ["template", "input", "arg", "edge", "store"]`——「兜底臂跳过」与「M1 词表层定罪」同时在场，缺一侧即红。
  - `ctx_from_in_vocab_is_judged_not_swallowed_by_fallback`（负例）：`from: template` 缺 template_ref → 恰 `["E2301"]`（逐字）；带 template_ref → 干净。SECTION_FROMS 与 inv1 match 臂不同步（新增第六词只改一侧）即由绿转红——语义行「静默吞段」风险的漂移闸。
  - `inline_primitive_ports_synthesize_kind_defaults`（m4.rs:393 正例，逐臂）：四臂（string `inputs.p == ''` / integer `== 0` / number `== 0.0` / boolean 裸绑定）全放行——合成默认值进入 inputs 绑定可求值为布尔。
  - `inline_primitive_non_boolean_synth_is_e1053`（负例，逐臂钉型）：非布尔三臂裸绑定 → `["E1053"]` 且逐字 `node 'n2' assert CEL 谓词求值非布尔：结果为 {string/int/float}`。**如实登记**：Boolean 臂合成值为布尔、裸绑定恰是合法谓词，无 E1053 负形——由正例钉死（合成错型则正例红），不编造第四条负例（文件头注+closure_note 双登记）。
- **`ci/branches.yaml` v1**：10 分支全 covered（v0 的 8 covered + 2 缺口闭合）；两条 `closure_note` 记录闭合路径与「v0 正例 assert_inputs_binding_from_incoming_edges 走 Named 臂、不逐臂踩内联四臂」的如实标注；footer 摸排统计逐项更新（25 fixture 条目、锚 10/10、名单 25/25）。
- **`ci/branches_check.py`**（SPEC ⑧.3 明示属实施批产出）：R0 schema/scope、R1 锚三元组、R2 夹具名单+path:line 对账、R3 pending 治理+宽限窗、R5 不产码说明；R4 coverage nightly 面（报告在场才判：登记分支正例在场但覆盖计数全 0 → 红；格式不可识别 → WARN 不误红；逐支裸分支对账待真实 llvm-cov 输出校准后升红——SPEC ⑧.1/⑧.5 两阶段设计）。容错点实录：cargo `--list` 的 `Running <target>` 段头走 **stderr**、`name: test` 走 stdout，必须合并捕获（首版只收 stdout 致 26 项全误红，已修并复测）。

## 三、D2 交付物与设计要点

- **`ci/xcheck.py`**：五组对账（4 硬 + 1 advisory）+ `[LEGACY] codepoints` 遗留登记行。与草案的差异及理由：
  1. **门数对账分基础/G0 两层**：基础「九门」口径硬对账（主张==基础硬门+advisory 且 step1-8 连续）；run.sh 出现 step10-14 时按 G0-A 勘误行（AGENT.md:91「九门+G0 五门=14 step」）硬核算术——未齐（<5/5）只发 advisory（合流前不误红），齐而不符 → 硬红（合流后不失守）。
  2. **门计数识别 bespoke 块**：硬门 = `^step N "` 助手行 ∪ `^echo "== stepN` bespoke 行（减 advisory 行）——step14 因退出码 2 须落 WARN 行而用 bespoke 块，只数助手行会让新门对账眼盲（本机制自身的逃逸形态）。advisory 判定排除连字符复合词（step7「advisory-db」是漏洞库不是 advisory 门——误分类会门数账失真，实录已修）。
  3. 仓内侧缺省路径 `__file__` 推导（root 钉死本工作树）；obs-dates 文书缺席 → 红（缺文件=未达成，复盘 C7 口径），不静默跳过。
- **`ci/xcheck_probe.sh`**：四 mutation 全在 mktemp 副本上跑（正本零改动，SPEC ⑤.5）；sed 未命中（措辞漂移）⇒ 副本无异 ⇒ 探针不红 ⇒ 脚本报 FAIL——探针自身失效也红，不静默放过。定位为 standalone 工具（不并入 step14 常跑面；由 B3 mutations 框架或人工复检调用）。

## 四、D3 交付物与设计要点

- **`ci/criteria.yaml`**：G4 四判据回填（门终裁原文）+ G0 两判据（阶段计划 §2.1/§2.2）+ G0-OBS-1 观察窗（C7/PP5 六要素+第七要素 env_guard）。**唯一数值勘误 G4-4 分母 12→13**：门终裁原文（2026-10-25）「12 判据全过」，红队 W3-C38 新增 R8 解析对账判据（随 3e2194c/e6e05a0 合流）后 step8 实数 13——证据 `ci/out/step8.log:25` `== step8 汇总：PASS=13 FAIL=0（判据 C1/I1/I2/I3/I4/R8/P-A/P-B/P-C/S1/R7/Z0）`；漂移登记写进 count_rule，不把已知失真数字留在机读正本。
- **`ci/criteria_check.py`**：六要素/观察窗要素/软上限/反空过条款字面核/waiver_adr 可解析（SPEC ⑧.2 裁决点 1 实施批动作）；清单机生成带「条目数==criteria 数」防御断言；ADR 指针解析根缺省=校验脚本所在仓库根（criteria 可能是 /tmp 探针副本，以其位置推导会把在册 ADR 误判悬空——实录已修）。数值不复算（归 step12 numbers.json，SPEC ⑧.3 分工边界）。

## 五、run.sh step14 段（隔离亲跑证据）

插入点：step9 块结束（`cat "$OUT/step9.log"`）后、汇总块前（SPEC-1 ② diff 草案锚点）。bespoke 块（对齐 step9 先例）而非 `step()` 助手——退出码 2 必须落 SUMMARY 显式 WARN 行不改 OVERALL，助手只表达 PASS/FAIL 两态（复盘 C3「禁两态死代码」教训）。门计数可见性按 step9 先例以 `echo "== step14 …"` 行登记。

| # | 验证 | 命令 | 输出摘录 | 结论 |
|---|---|---|---|---|
| 1 | 语法 | `bash -n ci/run.sh` | 无输出 exit=0 | PASS |
| 2 | WARN 路径（三脚本聚合 rc=2） | 隔离 harness（抽取 machine_batch 函数体 + 模拟 AF_ROOT/OUT/SUMMARY/OVERALL，CARGO_TARGET_DIR=afbuild-g0d） | `模拟 SUMMARY: step14 PASS-WITH-WARN 2903ms` / `模拟 OVERALL=0 rc=2`；step14.log 内 `== 14 子检查退出码：branches=0 xcheck=2 criteria=0` | PASS |
| 3 | FAIL 路径（删 covered 负例，SPEC-1 ⑤.3 集成层） | 同上 harness + branches_check 指向删负例副本 | `模拟 SUMMARY: step14 FAIL 352ms` / `模拟 OVERALL=1 rc=1`；`[FAIL] LOOP-E2002-MAX-ITER-POSITIVE: covered 缺夹具（正例 1/1、负例 0/1）` | PASS |
| 4 | 时长（step3 已构建后） | 同 #2（两次） | 2903ms / 2461ms（branches_check 自跑 `cargo test --workspace -- --list` 为主，实测 88 个测试文件段） | PASS（常跑面 <5s 预算内） |
| 5 | 异常退出不静默放行 | machine_batch 聚合逻辑（for rc in …; case 0/2/\*) | 非 0/2（如 python 崩溃 exit 1 以外码）→ hard=1 → return 1 → FAIL+OVERALL=1 | PASS（代码路径推证+FAIL 路径亲跑间接覆盖） |

## 六、靶向检查汇总（本会话亲跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| py_compile ×3 | `python3 -m py_compile ci/branches_check.py ci/xcheck.py ci/criteria_check.py` | exit=0 |
| bash -n ×2 | `bash -n ci/run.sh && bash -n ci/xcheck_probe.sh` | exit=0 |
| YAML 解析 ×2 | `yaml.safe_load(ci/branches.yaml)`（10 条目）/ `yaml.safe_load(ci/criteria.yaml)`（2 gates） | OK |
| af-dsl 新夹具 | `CARGO_TARGET_DIR=/home/pan-ding/afbuild-g0d cargo test -p af-dsl --test branch_fixtures_test` | 4 passed / 0 failed |
| af-dsl fmt | `CARGO_TARGET_DIR=… cargo fmt -p af-dsl -- --check` | 干净（首版 1 处格式 diff 已 fmt 修复，fn 行号未漂移：55/73/113/134 复核） |
| af-dsl clippy | `CARGO_TARGET_DIR=… cargo clippy -p af-dsl --all-targets -j4` | 零 warning/error（不新增 step2 基线外警告） |
| branches_check 正本 | `python3 ci/branches_check.py`（自跑 cargo 名单） | exit=0，10/10 OK，2.9s |
| xcheck 正本 | `python3 ci/xcheck.py` | 硬项 4/4 过，exit=2（advisory：头注漂移） |
| criteria_check 正本 | `python3 ci/criteria_check.py --emit-checklist ci/out/final-checklist.md` | exit=0，6==6 |
| 探针脚本 | `bash ci/xcheck_probe.sh` | 4/4 红+对照臂，exit=0 |

## 七、未做/已知限制（如实）

1. **未跑全量 `ci/run.sh`**（开发期纪律，合流阶段统一跑）；step14 段以隔离 harness 亲跑三条路径（WARN/FAIL/时长），全量管道的 step1-9 交互留给合流门。
2. **合流批集成项（跨组，须 g0merge 处理）**：G0-A 的 step12 `numbers.json` 门数复算命令为 `grep -cE '^step [0-9]+ \|^if fv_advisory' ci/run.sh`（见 af-wt-g0a `RESULT-G0A-对账三件套-20261028.md` §〇），**不识别 bespoke 回声门**（step14 无 `^step N "` 行）——合流后该复算会得 13 而口径为 14。建议合流时把 step14 的 `echo "== step14` 行纳入复算面（改正则添加 `\|^echo "== step`）或改由 xcheck 门数对账单源（SPEC ⑧.4 numbers.json 并轨议程）。本批不改 G0-A 文件（越界）。
3. **run.sh 头注「六步验收管道」仍未勘误**（xcheck advisory 每跑提示）：头注在 run.sh:3-28 注释区、非本组落点（step14 新增段以外不动）；G0-A 在其树已改「十二步」，合流时按 14 step 口径统一。
4. **nightly llvm-cov 面**：branches_check 的 coverage 对账按容错解析实现（登记分支零覆盖→红、格式不可识别→WARN），**未经真实 llvm-cov `--show-branch-count` 导出校准**（仓内无该产物）；逐支裸分支对账（SPEC ① 规则 1）按 SPEC ⑧.1/⑧.5 两阶段设计留待 nightly 首跑校准后升红。合成报告探针（M7/M8）只验证解析器逻辑，不背书真实格式。
5. **obs-dates 组耦合工作区文书**（AGENT.md/方案/四观察期文书住 `~/workspace/agent黑灯工厂`，SPEC 设计如此）：af 仓异地克隆时该组将红（缺文件=未达成）；默认路径可 `--doc-*` 覆写。
6. **numbers.json 单源并轨未接**（SPEC ⑧.4）：xcheck 现直接数 run.sh 实数；step12 落地后（G0-A 已在其树产 numbers.json，键为 `gates.ci_steps`/`gates.ci_hard`/`gates.fv_advisory`）可改为消费单源，属合流后演进。
7. **ci/out 再生产物未提交**（branches-report.txt/xcheck-report.txt/final-checklist.md）：CI 每次再生，入档会造成无谓 diff；证据以本报告命令+摘录为准。

## 八、锚点与依据

- 工单判据：`计划/研究组/冲刺批-20261027/验收判据.md` G0-D 节（4 条）。
- SPEC：`计划/研究组/G0-CI机制批-实施规格-20261027/D/1-branches/SPEC.md`、`D/2-xcheck/SPEC.md`、`D/3-criteria/SPEC.md`（§⑤ 验收判据逐条对照）。
- 总对账：`G0-CI批-总对账.md` §二（step14 槽位编号统稿）/§六.2（3/4 交卷口径）/§六.0（root 钉死）。
- 复盘：`红队复盘-逃逸分析与防再发-20261027.md` C2 prevention-5、C4 升级⑤、PP2 升级①②、PP5 升级②、C7 升级②、§5.1 枚举行。
- 交叉事实：G0-A 勘误行（`~/workspace/agent黑灯工厂/AGENT.md:91`，「九门+G0 五门=14 step」）；step8 判据实数 13（`ci/out/step8.log:25`）；af-dsl 代码锚（inv1.rs:193/333/337、shape.rs:43、loop_guard.rs:26/33/41、m4.rs:263/393、bind.rs:314）本树亲测。
