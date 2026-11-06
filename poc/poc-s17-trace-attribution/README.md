# POC · S17 离线轨迹归因批面（D-55 / st-b-C073 跨面，§七.2）

- 卡身份：wave-02 卡 `s17-trace-attribution-batch`（`30-工件/波次自转器-20261106/wave-02.json`；
  债号 **D-55**，父债 P3-B1；`计划/阶段欠账台账.md:118` 已登记〔未开始→本批合线后由 runner 翻态落账〕；
  通道 local-api（step 子代理 ssh-anolis 走门，本机永禁 cargo）；`allowed_paths ["tools/","af/poc/"]`）。
- 设计稿依据：`30-工件/strategies17-供给面设计-20261106.md` §七.2（跨面判据+卡字段草案）、
  §四#7（st-b-C073 判 ③ 跨面 / >L）。策略正本 `strategies/st-b-C073.yaml`（三态 `not_applicable`，
  **本批不动三态**——只建离线批面 + 标注数据面，非运行图）。
- **判据命令零改**：`ci/strategies_check.py`、`ci/criteria.yaml`、`ci/baselines/gate-inputs.sha256`
  一律未碰（dev 型 bot 卡禁项同款纪律，制度 §六.4；rt-e82/83 双案「判官/门/判据件一改即拒收」）。

## 一、跨面判据（为何不是运行图）

st-b-C073 的输入是**已完结轨迹集合**（离线批处理面）+ **人工对齐率标注**（数据/人工面）；
AFB 是单次运行蓝图，无离线批处理分析语义（na_reason 原文）。失败模式判官臂 `route.judge@1`
已注册（`af/packages/components.json`），缺的是**批调度 + 标注数据面**——本件即补这条腿，
产物=确定性离线批工具 + 对齐率标注夹具 + 实跑读数证据，**零 crates/ 源码、零组件注册**。

## 二、导出规约（records.jsonl / mes，逐行 JSON object）

批工具 `tools/trace_attribution_batch.py` 只读轨迹集合，识别三种行形状：

| 形状 | 文件 | 判识键 | failure 信号 | 归属 |
|---|---|---|---|---|
| **result 行** | `results.jsonl` | `task_id` 且（`suite`\|`error_code`\|`verifier_note`） | status / error_code / verifier_note / reward(verdict) | 任务级终局，**失败模式主载体** |
| **record 行** | `records.jsonl` | `item_id` 或（`node_id`&`plan_id`） | verdict / gate / status / note（`record.rs:213-244` 形状） | 节点级执行读数 |
| **mes 行** | `mes-events.jsonl` | `work_order_id`&`kind` | 生命周期（release/start/complete/close，实测仅此四种） | 按 WO 聚合，reached complete\|close ⇒ 一条 resolved 轨迹（只贡献分母） |

行形状无法识别 ⇒ 计入 `unparsed`，**不静默当轨迹**。reward/verdict 的 `null`=供给面失败
（`crates/af-execline/src/pkg/sweng.rs:680-681`），非未知。

## 三、失败模式判官（三值闭集，禁增删）

闭集（st-b-C073 / S067 固定）：`tool_usage_error` 工具使用错误 / `syntax_error` 语法错 /
`other` 其他。判据优先级（可审计）：

1. **显式工具错误 status**（`tool_error`/`unit_error`/`tool_call_failed`/`bad_tool_args`）⇒ tool_usage_error；
2. **结构化 EX 码**（`EX_CODE_CLASS`，依据 `crates/af-execline/src/error.rs:87-95` 语义亲读）：
   - EX4001–4006/4008（计划装订/消费面结构·契约·模板·图形态错误）⇒ `syntax_error`
     （AFB 轨迹对 S067「语法错」的同构映射：蓝图本身语法/结构错）；
   - EX4007（预算熔断）/EX4009（节点墙钟超时）⇒ `other`；
3. **status/note 关键词三分类计分取最大**（平分/无命中 ⇒ `other`，保守不当类）：关键词均为特定短语，
   ASCII 词走 `\b` 词界、CJK 走子串——selftest ⑧ 亲测裸 `digest` 曾把 `args_digest` 咬成语法错，
   故禁裸单词。

EX 码优先于自由文本；关键词仅兜底无码失败（tool_usage_error 无专用 EX 码，靠 note/status 判）。

## 四、聚类分布报表（report 模式）

`population{total_traces,resolved,failed,indeterminate}`（indeterminate 保证 total 恒等式不丢轨迹）
+ `failure_distribution{三类 count+share_milli}` + `by_error_code` + `by_source` + `failed_traces`。
占比用 milli 整数（禁浮点漂移）。**零轨迹（源在场但无可解析轨迹）= fail-closed 红 rc=1**（acceptance ①）。

## 五、人工对齐率夹具（align 模式，数据/人工面）

夹具=`annotations/alignment-fixture.jsonl`（`{trace_id, human_label∈闭集, annotator, rationale}`）。
对每条标注轨迹跑同款判官，算 judge-vs-human 一致率 `alignment_rate_milli` + 逐类一致 + 混淆矩阵 +
分歧清单 + 孤儿（collection 无匹配失败轨迹）。**无标注（夹具缺件/空/零有效标注/全孤儿）= fail-closed 红 rc=1**
（acceptance ②）。可选 `--min-align-milli` 设对齐率下界（默认 0=仅要求夹具在场）。

夹具含**刻意对抗例** `fix-amb-1`（agent 选错检索工具，措辞「选择」不在判官关键词内）——
human=tool_usage_error 而规则判官判 other，用于证明夹具具备咬合力（量出判官盲区，非橡皮图章 100%）。
LLM 判官（`route.judge@1`）上线后同理用本夹具验证对齐率。

## 六、验收逐条对账

| acceptance | 判据 | 实测证据 | rc |
|---|---|---|---|
| ① 批脚本对既有跑批读数实跑落分布（零轨迹=fail-closed 红） | 读 `poc-p3-a1` 既有 records+results | `readings/report-real-p3a1.json`（3 轨迹/1 resolved/2 failed，EX4005+EX4006 均 syntax_error）；负例 `raw/negative-zero-trace.txt` | report 0 / 零轨迹 **1** |
| ② 对齐率标注夹具在场（无标注=红） | 夹具 corpus + alignment-fixture | `readings/align-fixture.json`（labeled=10，对齐率 900 milli，1 分歧 fix-amb-1）；负例 `raw/negative-no-annotation.txt` | align 0 / 无标注 **1** |
| ③ 判据件/门零改 | git 对账 + strategies_check | 见 §七 | 0 |

判别力自证：`tools/trace_attribution_batch.py --selftest` ⇒ `SELFTEST_VERDICT=BATCH_BITES`（8/8 PASS，
子进程跑本体；`raw/selftest.txt`）。含：干净集合绿/零轨迹红/无标注红/空标注红/孤儿标注红/源缺件 rc=2/
三类判官判别力。

## 七、判据件/门零改对账

- 本批仓面改动**仅** `tools/trace_attribution_batch.py`（新增）+ `poc/poc-s17-trace-attribution/**`（新增）；
  未改任何既有文件（故无 `.bak`——`.bak` 纪律只针对被改文件）。
- 判据件 `ci/strategies_check.py` / `ci/criteria.yaml` / `ci/baselines/gate-inputs.sha256` 及
  `ci/run.sh` 门、`strategies/` 三态、`af/packages/` 注册表、`crates/` 源码**零改动**（`git status` +
  `git diff --stat` 对账，见下）。
- 门绿（anolis 走 cargo，本机禁）：`python3 -B ci/strategies_check.py` ⇒ exit 0（四行 INFO 全绿；
  计数 compilable=44/needs_syntax=0/not_applicable=17 与本批基线一致——本批不触及三态）。

## 八、复跑与指纹

- 复跑：`bash poc/poc-s17-trace-attribution/run-readings.sh`（cd 仓根、全相对路径、零网络、只读）。
- 分支 `feature/s17-trace-attribution-batch`（base `origin/product/main` = `04b5a5ea`）。
- 关键读数：真实 p3-a1 分布（`readings/report-real-p3a1.json`）；对齐率 900 milli
  （`readings/align-fixture.json`）；corpus 全量 16/6/10（`readings/report-corpus.json`）。
-  taxonomy 映射锚：`crates/af-execline/src/error.rs:87-95`（EX 码语义）、
  `crates/af-execline/src/record.rs:213-244`（record 行形状）、`sweng.rs:680-681`（null=供给面失败）。
