# 口径-EXP1-repeats-v1 —— EXP-1 repeat 轮（A0/A1/A2 第二跑 72 单）执行与分析口径

> 件号：P3-B1 残项（EXP-1 repeats 72 数据补齐）
> 落点：`af/poc/poc-exp1-repeats/`（af 仓分支 `feature/exp1-repeats-backfill`）
> 预注册正本（**只读，零改动**）：`计划/研究组/B线核心批-20261029/experiments-v1.json`
> （EXP-1 段）、`exp1-allocation.json`、`exp1-materials.json`、`exp1-lock.json`
> 判定程序唯一实现（**不复制、不改写**）：`tools/b-verdict-unique.py`（工作区）

---

## 一、件的定位与边界（一句话）

把 EXP-1 预注册里已登记但从未执行的 **repeat 轮 72 run**（A0/A1/A2 各 24 任务第 2 次）
补跑成数据，并给出**flakiness（run 间抖动）与单轮不收敛任务名单**——这是 repeat 轮在预注册
里的**唯一文字用途**（`experiments-v1.json` EXP-1.`randomization.repeats`：「A0/A1/A2 三臂各
24 任务跑第 2 次（估 flakiness）；剂量臂不重复」）。

**本件不签发、不改写 EXP-1 主判定。** 主判定（`inconclusive`）由主轮 144 单经
`tools/b-verdict-unique.py` 唯一判定程序产出（落 `exp1-aggregate-summary.json`），本件不动它。
把两轮 pooled 并入主判定 = **改判据**，按预注册 `preregistration.change_rule`（「判据任何改动
= 新 exp_id（EXP-1b），旧判据结果不得改写」）须另开 **EXP-1b**；本件只把 pooled 读数作为
**探索性（探索性标签）**数据落盘，留作 EXP-1b 的输入，不据此作任何判定。

## 二、预注册绑定（逐条，只读）

| 预注册字段（experiments-v1.json EXP-1） | 值 | 本件执行面如何落 |
|---|---|---|
| `randomization.repeats` | A0/A1/A2 三臂各 24 任务跑第 2 次（估 flakiness）；剂量臂不重复 | 臂集合取 A0/A1/A2；A1d25/A1d50 不进本轮 |
| `sample_size.repeats_runs` | 72 | 本轮包数 EXPECT_PKGS=72（分配表 `round=="repeat"` 恰 72 行） |
| `sample_size.runs_total` | 216（main 96 + dose 48 + repeat 72） | 本轮补的正是缺的 72 → 216 全额到位 |
| `randomization.seed` | 20261029 | 包序沿用 `pkg-order.txt`（分配表顺序）过滤 A0/A1/A2；bootstrap 种子同源 |
| `execution.command_template` | `af-order submit --unit report --task <pkg> --profile glm-cp --data-root <root> --host local --max-turns 40 --spend-cap-cents 300` | 提交脚本逐字沿用（另补 `--spec/--state` 定位旗标与 `--workspace/--target-dir` 引擎定位旗标，与主轮同款实证命令一致） |
| `execution.zero_code_change` | true | 本轮不改 af 任何 Rust/DSL 代码；只在 `af/poc/` 下新增件 |
| `execution.task_package` | `<root>/<pkg>/{task.json,plan.json}`；plan.json 冻结复用 | 提交前逐包校验 plan.json sha256 == 冻结锚 `d841f1b6…52448`（144/144 全命中才放行） |
| `metric_operationalization.binarization.selected` | `all_hit` | 分析面沿用同一二值化 |
| `metrics[].source` | citation_unit.py 的断言 | 复算语义照 `af/packages/reportkit/units/citation_unit.py`（本仓正本）逐字重算 |
| `budget` | 3¢/run 保守估、`usd_cap=30` | 本轮 72 run ≈ $2.16（保守估）／折算 ≈$2.3，远低于 cap |
| `preregistration.change_rule` | 判据任何改动 = 新 exp_id（EXP-1b） | 本件零判据改动；pooled 只作探索性落盘 |

## 三、执行面口径（本轮如何跑）

| 项 | 值 | 与主轮的差异与理由 |
|---|---|---|
| 臂集合 | A0 / A1 / A2（各 24 任务 = 72 包） | 剂量臂不重复（预注册明文） |
| 任务包 | `/data/bench/exp1-pkgs/{A0,A1,A2}/<paper-id>/`（**复用主轮同款包**，不改 task.json/plan.json） | 材料件同源 ⇒ 两轮差异只来自「跑第 2 次」而非材料漂移 |
| 包序 | `pkg-order-r2.txt` = 主轮 `pkg-order.txt`（sha256 `41e8e265…c28d7`，与 anolis 副本逐字一致）按臂过滤 | 分配表顺序（预注册随机化序），可复算 |
| 订单号 | `E1R-<arm>-<paper>` | **不用 `E1-` 前缀**：主轮聚合器 `tools/b-exp1-aggregate.py` 的订单 glob 是 `E1-*`，若用 `E1-<arm>-r2-<paper>` 会被它误吞并触发 fail-loud（`arm=None 不在闭集`）；`E1R-*` 不匹配 `E1-*`，两轮镜像天然隔离（EXP-2 repeat 轮用的是 `-r2` 内缀写法，此处为隔离性改用独立前缀——**实现细节，不改任何判据要素**） |
| 状态根 | `/data/bench/exp1-state-r2`（独立于主轮 `/data/bench/exp1-state`） | 两轮工件物理隔离，镜像各自只读回传 |
| line 绑定 | `bline-exp1` / `1`（沿用主轮） | 同产线、同档位；轮次差异由 order_id 承载（EXP-2 repeat 轮同款做法） |
| profile | `glm-cp`（GLM-5.3-Flash @ open.bigmodel.cn coding plan） | **不得静默替换模型档**（主轮纪律）；`zero_code_change=true` 亦不许改 key_cmd（见 §六 被否决方案） |
| 并发 | 4（波次制度 `anolis_concurrency=4`） | 与主轮同档 ⇒ 墙钟口径可比 |
| 单包超时 | 1800 s | 主轮同款 |
| 密钥 | key_cmd 由引擎内部执行；提交脚本只跑 keycheck 报**长度**（`KEY_OK len=49`，零明文） | 主轮同款纪律 |

## 四、分析面口径（本轮读什么）

聚合器：`exp1-repeats-aggregate.py`（本件）。

1. **逐单提取**：results.jsonl 末行（status/reward/gate/turns/tokens/verifier_note）、
   MES `mes-events.jsonl` 的 release→close 墙钟、run 工件 `verify.log`（判官输出）+
   `report.json` + `sources/`（引用逐字复算）。
2. **真实性锚（fail-loud）**：执行集 ⊆ 预注册 `round=="repeat"` 分配表；快照 sha256 与
   `exp1-materials.json` 预登记 `paper_sha256` 逐字比对；`citations_total` 三锚一致
   （verifier_note ↔ verify.log ↔ report.json）；quote failures 复算一致；reward ↔ 双判官一致。
   任一条不过 ⇒ 整批拒绝聚合（判据先于数据，数据不自证）。
3. **覆盖对账**：预注册 repeat 72 行 vs 实跑订单目录，缺失/越界逐条点名。
4. **数据门**：沿用预注册常量（配对集 <20 条 / 单臂缺 report >4 条 → GATE），对 repeat 轮内
   A1−A2 / A1−A0 / A2−A0 三对逐条留痕。
5. **repeat 轮内统计（描述性）**：单臂 all_hit 率 + Wilson95、逐对配对差 / McNemar 精确单侧 p /
   TOST CP90 / 聚类 bootstrap 95% CI。**这些是第二轮的单独读数，不并入主判定。**
6. **flakiness（预注册文字用途的正面交付）**：主轮 × repeat 轮同 `(arm, task)` 配对，逐项比对
   `all_hit` / `reward` / `gate` 是否翻转、`tokens` 比率（r2/r1 的均值/中位/区间），并给出
   **单轮不收敛任务名单**（两轮 all_hit 或 reward 不一致者）。这正是
   `exp1-结果判读-v1.md` §3 所记「flakiness 无法估计，单轮不收敛任务无从识别」那条偏差的补齐。
7. **探索性 pooled 读数（明确不作判定）**：两轮合并成 2×n 观测，配对差 + **任务聚类** bootstrap
   95% CI（识别轮间相关），并标注「并入主判定＝改判据 ⇒ 须开 EXP-1b」。
8. **成本**：tokens 合计 × 2¢/17,935 tok 折算（EXP-1.budget.basis 同源基座）；`cents_spent`
   按主轮口径如实登记 `not_recorded`（订单 ledger/records 无记账行），**不补估**。

### 口径同源自证（可复跑）

本件聚合器与主轮聚合器 `tools/b-exp1-aggregate.py` 是两套代码，但公式同源。为证明「同源」，
本件提供对拍模式：用**同一份主轮镜像**跑本件实现，与主轮**已发布**汇总
`计划/研究组/B线核心批-20261029/exp1-aggregate-summary.json` 逐项比对：

```bash
python3 -I af/poc/poc-exp1-repeats/exp1-repeats-aggregate.py --selfcheck-only \
  --regress-main 计划/研究组/B线核心批-20261029/exp1-aggregate-summary.json \
  --main-mirror ~/exp1-raw/mirror/orders
```

对拍项：6 臂 × {n_with_report, n_all_hit, all_hit_rate, citations_total_sum, tokens_total_sum}
+ 5 对照 × {n_paired, diff_pp, b, c, McNemar p, TOST 下界, TOST 上界}
+ {n_orders, n_with_report}。**全部 PASS 才允许用本件聚合 repeat 轮读数。**

## 五、成本口径（补跑 72 单的账）

| 项 | 值 | 依据 |
|---|---|---|
| 单均墙钟（实测基座） | 436.3 s/单 | 主轮 144 单 MES release→close 合计 62,823 s（`exp1-结果判读-v1.md` §7） |
| 72 单串行墙钟 | ≈31,411 s ≈ 8.7 h | 436.3 × 72 |
| 72 单并发 4 墙钟 | **≈2.2 h**（≤ 4 h 门 ✓） | 同上 ÷ 4；主轮 144 单并发 4 实测 ≈4.4 h，量级一致 |
| 模型调用 | 144 次（reader+writer 各 1） | 预注册 `budget` 口径 |
| 成本（3¢/run 保守估） | 72 × 3¢ = **$2.16** | `EXP-1.budget.cents_per_unit=3`、`usd_cap=30` |
| 成本（tokens 折算） | ≈$2.3（主轮同比例：4,162,616 tok ≈ $4.64 / 144 单） | 基座 2¢/17,935 tok（EXP-1.budget.basis） |
| 磁盘 | 72 单 ≈ 30 MB（主轮 144 单实测 57 MB） | `/data` 余量 8.9 GB ≥ 2000 MB 门 |
| 内存 | 并发 4，af 仓波次制度 anolis 配额 45 G 内 | 波次制度 §五 |

## 六、被否决的替代方案（登记，防止后续重犯）

| 方案 | 否决理由 |
|---|---|
| 换模型档（如 minimax 的 mavis-m31f/m3）补跑 | ① 预注册 `execution.command_template` 钉死 `--profile glm-cp`，主轮纪律「不得静默替换模型档」；② EXP-1 结论作用域明写「报告产线 + **glm-cp 档**」，跨档位数据与主轮不可比 ⇒ flakiness 估计被模型档差异混淆；③ 本轮目标是**补齐预注册的同一实验**，不是另起实验 |
| 换 zhipu 账号/密钥（同模型不同 CodingPlan） | 需改 `crates/af-execline/src/profile.rs` 的 key_cmd，违反预注册 `execution.zero_code_change=true`；且属「换档」范畴，须实验负责人裁定 |
| 用 minimax 档做「EXP-1b 预跑」 | 可以，但那是**新 exp_id 的新实验**（须先开 EXP-1b 并重跑预注册），不在本卡范围；登记为后续项 |
| 把两轮 pooled 并入主判定抬 n_p | 改判据 ⇒ 须新 exp_id（EXP-1b），本批不并入、不作判定（§一） |

## 七、偏差登记（实测，非推断）

### 偏差①：glm-cp 通道配额触顶，repeat 轮在 00:52–00:57 试跑被 429 全量拒绝

- 事实：按本件提交脚本在 anolis 起跑 72 单，前 8 单（00:52:53–00:57）全部失败，失败模式为
  端点 HTTP 429，provider 错误码 **1310**，报文：「您已达到每周/每月使用上限，您的限额将在
  **2026-11-06 14:16:36** 重置」。
- 证据件：`evidence/attempt1-quota429/`（16 份 `.result` 失败结果 + 提交日志）与
  `evidence/attempt1-quota429/probe-ledger-error.txt`（单单探针的账本错误原文）。
- 性质：**通道配额窗**问题，不是 harness 缺陷，也不是实验设计问题。
  - harness 在位（亲验）：`af-order`/`af-engine` 二进制在位；144 包在位且 plan.json 144/144
    命中冻结锚；keycheck `KEY_OK len=49`（零明文）；提交脚本 7 项预检全过（预检日志见
    `evidence/attempt1-quota429/submit.log`）。
  - 聚合器在位且口径同源（亲验）：§四 对拍 **全项 PASS**。
- 处置：试跑工件整目录留证（`/data/bench/exp1-state-r2` 与 `-probe`），**不复用为数据**
  （无 report.json，不进读数）；正式批在配额窗重置后重跑，用**新状态根**
  `/data/bench/exp1-state-r2r`（旧根改名保留为 attempt1 证据）。
- 复跑触发条件：配额窗重置（2026-11-06 14:16:36）后自动起批（§八）。

### 偏差②：本会话内数据不能落盘（时间窗事实）

- 事实：配额窗重置时刻在本件施工时刻之后约 13.3 h；72 单批自身墙钟 ≈2.2 h。
  ⇒ **数据落盘时刻 ≈ 2026-11-06 16:30 之后**，不在本会话的可达窗口内。
- 处置：§八 的自动化排期（起批 + 回传 + 聚合 + 落数据 + commit/push）已武装，
  不依赖任何代理在线；本会话内先交付「口径登记」件（缺口如实成文）与全套已就位 harness。

## 八、自动化排期（armed run；不依赖代理在线）

两段，均为常驻脚本 + 日志 + 标记文件（幂等、可重跑、可人工接管）：

1. **起批段（anolis）**：`exp1-repeats-armed-run.sh`
   - 等到配额窗重置（默认 2026-11-06 14:16:40 +0800，可用 `T0_EPOCH` 覆盖）；
   - 起 72 单批（并发 4）；**早停闸**：前若干单若连续命中 429 ⇒ 杀批、等待 30 min 再试，
     最多 6 轮；仍不可用 ⇒ 写 `GAP-MARKER`（缺口登记兜底），不空烧 72 单；
   - 批完写 `DONE-MARKER`（含汇总行数/Closed 数/墙钟）。
2. **落数段（本机）**：`exp1-repeats-land.sh`
   - 轮询 anolis 的 `DONE-MARKER`（最长 12 h）→ 回传只读镜像（tar）→
     跑聚合器（口径对拍先行，不 PASS 则拒绝落数）→ 写 `data/` 三件 →
     commit + push（显式 refspec，非 force）到 `feature/exp1-repeats-backfill`。

**兜底**：若 14:16 窗口仍不可用（例如被 burn-runner 抢占），起批段写 `GAP-MARKER`，
落数段写 `gap-登记-补跑窗口未开.md`，本卡按「缺口如实登记」收口（登记件模板已在本件 §七）。

## 九、复现命令（原件可复跑）

```bash
# ① 提交（anolis 上，pan-ding 身份）
cd /data/bench/exp1 && bash exp1-repeats-submit.sh
# ② 只读镜像回传（本机）
ssh anolis 'tar czf - -C /data/bench/exp1-state-r2r orders' | tar xzf - -C ~/exp1-raw-r2/mirror
# ③ 口径对拍（先决门）
python3 -I af/poc/poc-exp1-repeats/exp1-repeats-aggregate.py --selfcheck-only \
  --regress-main 计划/研究组/B线核心批-20261029/exp1-aggregate-summary.json \
  --main-mirror ~/exp1-raw/mirror/orders
# ④ 聚合（出读数）
python3 -I af/poc/poc-exp1-repeats/exp1-repeats-aggregate.py \
  --mirror ~/exp1-raw-r2/mirror/orders --main-mirror ~/exp1-raw/mirror/orders \
  --exp1-dir 计划/研究组/B线核心批-20261029 --out-dir af/poc/poc-exp1-repeats/data
```

## 十、本件**不做**的事（防止越界）

- 不改 `experiments-v1.json` / `exp1-allocation.json` / `exp1-materials.json` / `exp1-lock.json`
  / 主轮 `exp1-aggregate-summary.json` / `exp1-结果判读-v1.md`（预注册与主轮正本**零改动**）；
- 不改 `tools/b-exp1-aggregate.py` / `tools/b-verdict-unique.py`（判定程序唯一实现不动）；
- 不改 af 仓任何 Rust/DSL 代码（`execution.zero_code_change=true`）；
- 不改 EXP-1 主判定（不签发新 verdict，不把 pooled 读数当判定）。
