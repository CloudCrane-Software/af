# strategies/ — 共识策略可选库（libA-20261029）

本目录是 af 仓的**可选策略库**：把研究组策略册（共识策略）投影为可编译的库条目。
格式契约由工单钉死（两线一致）；本文件只说明读法与三态语义，不新增规范。

## 一、条目格式（每策略一个 YAML，`st-<簇id>.yaml`）

| 字段 | 语义 |
|---|---|
| `id` | `st-<簇id>`，与文件名一致（如 `st-C008` ↔ `st-C008.yaml`） |
| `name` | 策略名（策略册原文） |
| `tier` | `A`\|`B`（A=跨语料强共识；B=同语料多卡） |
| `claim` | 一句话声称价值（策略册 `assertion` 原文） |
| `claimed_value` | `{metric, direction: up\|down, context}`——声称价值的度量、方向与适用边界 |
| `sources` | 支撑卡 id 列表（策略册 `card_ids`） |
| `consensus` | `{corpora: 跨语料数, cards: 卡数}` |
| `experiment` | `{status: none\|inconclusive\|confirmed\|refuted\|pending, exp_id, note}`——实验元数据按实情填；无实验关联=`none`/`-`/`-` |
| `dsl_state` | 三态：`compilable`\|`needs_syntax`\|`not_applicable` |
| `dsl_snippet` | `compilable` 必填：多行 AFB 片段，**须能被 `af_dsl::load_str` + `validate` 零违例通过** |
| `needs_syntax_note` | `needs_syntax` 必填：缺什么语法 + 扩展示例 |
| `na_reason` | `not_applicable` 必填：为何不适用 |

三态判定纪律：**诚实优先**。能写出可过 `af-dsl` 全链校验的片段才记 `compilable`；
当前语法缺表达手段（如缺某一等构造/算子）记 `needs_syntax` 并给扩展建议；
策略本质不是图结构语义（如纯度量口径声明）记 `not_applicable` 并写明理由。
不得为凑数硬写片段——CI 门（`ci/strategies_check.py`，挂 `ci/run.sh` step14 段尾
14d）对 compilable 条目逐一编译断言，红即 exit 1。

## 二、本批范围（libA-20261029 + libB 合并）

`index.yaml` 为**并集登记簿**：A 档 11 簇（甲线 libA-20261029，`st-C0XX`）+ B 档 48 簇
（乙线 libB-20261029 提交 243fa0f，`st-b-C0XX`）共 59 条；`rank` 取策略册全局排序键
（跨语料数>卡数>引证密度）。登记值计数（`counts`）与条目文件实测值由 14d 门
（`ci/strategies_check.py` R2/R3）复核对账，漂移即红；磁盘条目未登记同样红
（全条目契约）。乙线提交明示 index/README/CI 归甲线合并——本目录即该合并面。

实验元数据说明：EXP-1（报告产线规则极性 A/B）2026-10-29 终裁 `inconclusive`
（主对比作用域；exp1-结果判读-v1.md:10/:151）。其预注册主断言卡 S161 属策略册
C122（C 档单卡）、EXP-2 主断言卡 S115 属 C103（C 档单卡），均不在 A/B 档 59 簇内；
A 档中唯一的判据类（评测方法论）策略 st-C007 的配对同模型 A/B 方法即 EXP-1 的
实例化，故其实验字段按实情记 `exp_id=EXP-1, status=inconclusive`（note 注明
其声称值本身未被 EXP-1 直接检验），其余 58 条无实验关联记 `none`。

## 三、新增条目

1. 复制同态条目改字段；`dsl_snippet` 先本地过
   `cargo test -p af-dsl --test strategy_library_test` 再提交；
2. 更新 `index.yaml` 的 `entries` 与 `counts`（登记值须与文件机算值一致）；
3. 跑 `python3 -B ci/strategies_check.py` 全绿后方可提交。
