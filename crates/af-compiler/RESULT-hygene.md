# RESULT-hygene — G2b 卫生债清偿简报（任务包 P2-Q03 · G2 放行附条件清除）

- 分支：`phase2-g2b-hyg`（自 `phase2-g2b@e85bfdb` 切出；工作树 `af-wt-phase2-order` 开工时在 `phase2-golden2`，`git status` 确认干净后切换）。
- 依据：**G2G3-放行裁决-20261024.md §一附条件**（G2 门放行附强制条件，三项）；证据底稿 af-compiler-G2b-独立验证-20261024.md（有条件通过）。
- 构建：`CARGO_TARGET_DIR=~/afbuild-phase2w`；磁盘开工 `df -h /` 242G 可用。

## 交付清单（三件）

| # | 件 | 交付 | 验证（实跑） |
|---|---|---|---|
| ① | fmt 全量归零 | `cargo fmt -p af-compiler` 一次收齐 13 个 `.rs` 文件（611 插入/183 删除） | 基线复现：`cargo fmt -p af-compiler -- --check` rc=1，`Diff in` 120 行去重 **117 处**（与独立验证 §7 计数一致；G2b 新增 49 + 基线既有 68）。清偿后 `cargo fmt --check -p af-compiler` **rc=0**（`cargo fmt -p af-compiler -- --check` 亦 rc=0，输出 0 行） |
| ② | RESULT-G2b.md 勘误 | 偏差表补登 #12-#15 四项 + 结构体计数 29→41 勘误注 | 逐项对码核实：`CompiledFrom.registry_digest`（plan.rs:81）/ `UnitBinding` 无 `selector`（plan.rs:164-167，契约 §7.1 `{unit_id, unit_version, selector}`）/ `NodeBudget` 数组元 `{id, dimension, cap}`（plan.rs:194-198，草案单对象形）/ `PlanArtifact.provenance`（plan.rs:209，草案仅 name/is_final/qualified_by）；41 型 = components 9 + bind 7 + plan 25 个 `pub struct`/`pub enum`（grep 实数） |
| ③ | RESULT 更新记录 | RESULT-G2b.md 新增「更新记录」节，注明本次清偿依据 = G2G3 放行裁决 §一附条件 | 见该节（本文件末次提交） |

**计数口径说明（②）**：放行裁决 §一附条件②表述为「三项偏差」，独立验证报告 §5.3 实列 **4 项**（a-d）且其 §9 整改清单称「四项未登记」——任务包明示「以 af-compiler-G2b-独立验证-20261024.md 所列为准」，故 4 项全数补登（#12-#15，含裁决点名的 `node_exec.unit` 缺 `selector`），偏差表 11 条 → 15 条。

## 零语义验证

- **测试原绿**：`CARGO_TARGET_DIR=~/afbuild-phase2w cargo test -p af-compiler` → rc=0，**53 通过 / 0 失败**（bind 17 + golden 11 + inline_body 4 + ir_schema 8 + lower_error 2 + plan 7 + route_guard 4；`grep -c FAILED` = 0）——含 11 个 golden 逐字节比对与篡改检测测试，可观测行为与 fmt 前一致。
- **非干扰**：`cargo test -p af-dsl` → rc=0，**244 通过 / 0 失败**。
- **diff 体检**：全量 diff 逐段目视为 rustfmt 机械动作（换行/缩进/大括号展开/尾随逗号）；非空白字符多重集逐文件核对——除 `,`（尾随逗号增补）与配对 `{`/`}`（块表达式折叠/展开，每文件增减平衡）外**零差异**，无任何标识符/字面量字符增减。
- **改动面**：仅 `crates/af-compiler/` 内 13 个 `.rs` 文件 + 2 个文档（RESULT-G2b.md、本文件）；`git status` 无其他改动；af-dsl、golden、testdata、components.json 零触碰。

## 纪律

- 未写 `计划/变更日志.md`；未触其他工作树；构建仅 `~/afbuild-phase2w`。
- 禁止项遵守：fmt 之外的代码改动 = 0（见上零语义验证）。
- 收工 `df -h /`：242G 可用。
