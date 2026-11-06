# POC · S17 判分可信度协议（strategies17 st-b-C022 · 债 D-57）

- 工件：卡 `s17-judge-reliability-protocol`（债号 D-57，父债 P3-B1；设计稿
  `30-工件/strategies17-供给面设计-20261106.md` §七.4）。
- 分支：`feature/s17-judge-reliability-protocol`（从 `origin/product/main`
  = `04b5a5ea` 显式 refspec `git fetch origin product/main:refs/remotes/origin/product/main`
  取回后切出）。
- 门跑点：anolis 工作树 `/data/bench/s17-judge-reliability-protocol-wt`
  （HEAD `3564bf1d`，与 origin 分支同点）；`CARGO_TARGET_DIR=/data/bench/targets/s17-judge-reliability-protocol`。
- 工件：`README.md`（本件）+ `raw/`（原始门读数）。

## 一、契约立场（本卡核心）

现 verdict 契约 = 二值 + null：`resolved: Option<bool>`，`None` = **供给面失败**
（锚 `crates/af-execline/src/pkg/sweng.rs:705-706`「resolved ∈ {true,false}=有结论；
null=供给面失败」，构造点 `:732`；执行面 `crates/af-execline/src/exec.rs:119-120`
「None = 无判定（供给面失败/未跑完）」）。〔设计稿 §七.4 记的 `sweng.rs:680-681`
是设计时 HEAD `2de44533` 的坐标；现 HEAD 已推进，按当前代码读准为 705-706。〕

「无标准答案评审门禁」缺的第三态是 **undecidable**（判分器跑了、但结论不可信：
多评委分歧 / 判分被污染）——它既非 pass/fail，也**不该**塞进 `None`（否则「不可信」
与「供给面失败」被混成一坨）。既有 `af_eval::GateVerdict{Pass,Fail,NeedsHuman}`
（`crates/af-eval/src/gate.rs:112-118`）已示范三值枚举，但消费端压回
`NeedsHuman → None`（`crates/af-execline/src/gate_unit.rs:471-474`、
`crates/af-execline/src/graph_unit.rs:2832-2834`）——即把「需人工」折成与「供给面
失败」同形的 null。本卡正要不重蹈此覆辙。

**本卡落点**（`crates/af-execline/src/pkg/reliab.rs`，新增模块，承 pkg/sweng.rs
模块结构先例）：
- `Decision{Pass,Fail,Undecidable}` 三值判定；`Option<Decision>` 外层 `None` 仍
  承载供给面失败——与 `resolved: Option<bool>` 的 null **正交**。
- `Decision::from_resolved`：`Option<bool> → Option<Decision>` **无损**搬运
  （`Some(true)=Pass / Some(false)=Fail / None 原样仍是 None`，绝不变 `Undecidable`）。
- **故意不提供** `Decision → Option<bool>` 回转：`Undecidable` 在 `Option<bool>`
  无忠实落点（只能撞 `None`）——这个「缺失」即本卡要守的边界；新消费面直接匹配
  `Decision`。
- `calibrate`：多评委校准（干净票 unanimity；污染票/供给面失败票不计；全供给面
  失败透传 `None`，全污染判 `Undecidable`）。
- `audit_contamination` / `judge_vote`：禁泄漏标样污染检测。
- `naive_majority`：污染无感的朴素多数决（负例夹具对照臂）。

不混淆既有 null 语义、不改 af-eval（allowed_paths 外）、不增 components.json
计数（st-b-C022 保持 not_applicable=17，属契约面扩展非策略投影）。

## 二、改动面（断言只增不减，零判据件改动）

| 文件 | 变动 |
|---|---|
| `crates/af-execline/src/pkg/reliab.rs` | 新增（206 行）：三值 + 校准 + 污染审计 |
| `crates/af-execline/src/pkg/mod.rs` | +1 行 `pub mod reliab;` + 文档 bullet（两模块→三模块） |
| `crates/af-execline/tests/pkg_reliab.rs` | 新增（208 行，15 测试） |

零改既有 verdict 契约行为（`SwebenchVerdict` / `verdict_from_verify` /
`ExecOutput.verdict` 字节不变）；判据件（`ci/strategies_check.py`、`ci/criteria.yaml`、
`ci/baselines/gate-inputs.sha256`、`ci/clippy-baseline.txt`）零改动。

## 三、门（anolis 实跑，exit 全 0）

S17 卡门集 = strategies_check + compiler/dsl test + workspace test（承
`poc-s17-registry-a` 三门先例；设计 §七.4 验收 ①②③ 同，未列 clippy）：

| 门 | 命令 | 读数 |
|---|---|---|
| strategies_check | `python3 -B ci/strategies_check.py` | exit 0；total=61 compilable=**44** not_applicable=17（44 零回归） |
| compiler+dsl | `cargo test -p af-compiler -p af-dsl` | exit 0；38 suites / **401 passed / 0 failed** |
| workspace | `cargo test -j4 --workspace` | exit 0；162 suites / **1638 passed / 0 failed / 9 ignored** |
| 装载逐条（pkg_reliab） | `cargo test -p af-execline --test pkg_reliab` | exit 0；**15 passed / 0 failed** |

基线对账（设计 §七.4 验收②「workspace test 不劣化，基线 1620/0/9，E9001 读数」）：
实测 1638/0/9 —— failed=0、ignored=9 与基线一致、passed 仅增（含本卡 15 条）⇒ 不劣化。

## 四、负例夹具（设计 §七.4 验收①）

`pkg_reliab.rs`：
- `negative_fixture_contamination_flips_false_pass_to_fail`：2 条泄漏票（判定文本含
  禁标样 `GOLD_PATCH`）伪造假 Pass 多数，1 条干净 Fail 被淹没 ⇒ `naive_majority`=Pass
  （方向性假结论），`calibrate` 丢弃泄漏票 ⇒ **翻转为 Fail**。
- `negative_fixture_contamination_downgrades_false_pass_to_undecidable`：1 条泄漏票
  tipping 出假共识，干净票实际 pass/fail 分歧 ⇒ `naive`=Pass（假），`calibrate` ⇒ **Undecidable**。
- 反混淆不变量：`calibrate_all_supply_side_failure_transparents_none_not_undecidable`
  （全供给面失败 ⇒ `None`，非 `Undecidable`）+ `calibrate_all_contaminated_is_undecidable_not_none`
  （全污染 ⇒ `Undecidable`，非 `None`）——钉死 `None ≠ Some(Undecidable)`。
- `decision_from_resolved_is_lossless_and_never_maps_none_to_undecidable`：旧契约→三值
  搬运的 null 保语义。

## 五、clippy 说明（如实登记，非本卡门集）

`cargo clippy -p af-execline --all-targets -- -D warnings -A clippy::collapsible_if`
⇒ **exit 0**（本卡代码 reliab.rs + pkg_reliab.rs 零告警）。

`cargo clippy --workspace --all-targets` 全量含一处 **base 既有**告警：
`clippy::collapsible_if crates/af-execline/src/span_produce.rs:515`（×2，来自 base
commit `6ef49c80`「P3-A2 单根 trace」，非本卡所改；`git diff origin/product/main..HEAD
-- crates/af-execline/src/span_produce.rs` 为空）。该告警在空基线
（`ci/clippy-baseline.txt` 0 数据行）下使全量 `-D warnings` / 基线门报红——**属 base
继承、非本卡回归**。clippy 不在 S17 卡门集内（设计 §七.4 验收未列 + `poc-s17-registry-a`
先例三门无 clippy 日志），本卡不改 span_produce.rs（越 card 主题 + 并行施工冲突风险）。

## 六、债号登记（如实登记）

D-57 截至本件未在正本 `计划/技术债登记.md` 登记（D-54…D-58 均未）；制度 §六.3
「台账无行的债号占用闸 fail-closed BLOCK」= 派卡前置，属编排层职责（sibling
`poc-s17-registry-a` D-58 亦未在施工树登记债行）。本施工员不越 allowed_paths 改台账，
留待主代理合线前登记。

## 七、指纹

- 分支 HEAD：`3564bf1d`（origin/feature/s17-judge-reliability-protocol 同点）。
- 提交链：`3dc71256`（feat 主体）→ `0d308d3a`（naive_majority 多数决 + 污染测试文本）→
  `3564bf1d`（删冗余 use serde_json）。
- anolis 门跑点 worktree `/data/bench/s17-judge-reliability-protocol-wt`；
  `CARGO_TARGET_DIR=/data/bench/targets/s17-judge-reliability-protocol`。
