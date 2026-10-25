# FARM-REPORT — 对抗样本农田：500 份 DSL 文档猎杀 af-dsl 验证器

- 日期：2026-10-24（任务当期）
- 农场根：`/home/pan-ding/.cache/afv1-farm/`
- 被测：`af-dsl` @ 工作树 `/home/pan-ding/workspace/af-wt-phase2-dsl`，
  branch `phase2-g2b`，commit `e85bfdbec78f141547d5e3c7df032aca76ee7b6b`（开工前
  `git status --porcelain` 为空；注：g3gate 的 Cargo.toml 注记 `@1786623` 已过时，
  该工作树此后有推进，本报告以实测 commit 为准）
- 结论速览：**500/500 全有裁定；异常 24 项立案**（panic 1、crash 4、漏网 10、
  误杀 4、复核存档 5），另文档化边界 5 项入档不立案。清单见
  `anomalies/ANOMALIES.md`（逐项附样本文件 + 复现命令 + 源码定位）。
  异常样本零修改、零提交执行，全部只入报告。

## 1. 生成分布（500 = 10 + 105 + 126 + 259）

| 策略 | 份数 | 说明 |
|---|---|---|
| A 合法基线 | 10 | 单节点/链/loop/map/human+route/escape 数据流/eval_input/双层 loop/loop⊃map/多 exit route |
| B 系统性变异 | 105 | B1 字段类型翻转 45 · B2 边界值 13 · B3 空集/缺段 12 · B4 嵌套深化 7（含 YAML 流深 5k/50k）· B5 Unicode 7 · B6 超长标识符 5（100KB id/类型名、5 万段写者路径、90 万段 CEL）· B7 YAML 怪癖 16（重复键/多文档/BOM/CRLF/tab/锚点/merge 键/tag 等 raw 文本） |
| C 定向攻击 | 126 | 路径别名 14 · 多写者 13 · 上下文装配 20 · INV-3 10 · INV-4 15 · route 12 · CEL 子集 29 · 逃逸 7 · 信封语义 6 |
| D 随机爆破 | 259 | seed=20261024，`random.Random(20261024)`，基线池=A01-A10，每份 1-3 次变异（flip/del/dup_elem/swap_ids/rewire/retarget/cel_junk/word/zero/enum_flip/port_rename），mutation 清单逐份入 manifest |

- 生成器版本：`afv1-farm-gen/1.0 (seed=20261024; pyyaml-6.0.3)`（Python 3.14.4）。
  重生成命令：`python3 ~/.cache/afv1-farm/gen.py ~/.cache/afv1-farm/samples`
  （确定性：同 seed 同字节；`manifest.jsonl` 逐份记录 expect/note/strategy）。
- 期望分布：expect=PASS 57 / expect=REJECT 132 / expect=None(探针) 311。

## 2. 执行 harness（~/.cache/afv1-farm，参照 ~/.cache/afb-g3gate 模式自建）

- crate：`farm/`（`afv1-farm`，只读 path 依赖 af-dsl 工作树，不入 workspace）。
  与 g3gate 三点差别：①每样本独立子进程（栈溢出/abort/死循环不连坐，20s 超时轮询）；
  ②`load_str` 与 `validate` 各自 `catch_unwind`，panic 归一为 `PANIC-CAUGHT` 记录
  （载荷摘要 160 字符、控制符转义）；③TSV 机器可读：file/verdict/layer/codes/nvio/panic/ms。
- 全量：`afv1-farm run samples results.tsv 8`（18 秒 wall）。
- 单样本：`afv1-farm one <file>`；逐条违例复现：`afv1-farm detail <file>`（g3gate 同款输出）。
- 构建：`cd ~/.cache/afv1-farm/farm && CARGO_TARGET_DIR=~/.cache/afv1-farm/target cargo build --release`
  （注：报告定稿时为释放磁盘配额已删除 target/，重建约需 1-2 分钟，依赖 crates.io 缓存仍在）。

## 3. 裁定总账（两轮全量一致）

```
PASS 198 | REJECT 297 | PANIC-CAUGHT 1 | CRASH-SIGNAL 4   （=500）
```

- 确定性验证：`diff <(cut -f1-6 results.tsv | sort) <(cut -f1-6 results_rerun.tsv | sort)`
  → 空（除耗时列外两轮逐位一致）。
- 期望核对：expect≠None 的 189 份中，仅 1 份与硬期望不符且为**有意 hunt**
  （C_AL14 误杀样本，见 ANOMALIES.md C 类）。
- PANIC/CRASH 全录：B60（validate panic）+ B89/C_CE15/C_CE16/C_CE19（SIGABRT），
  形态与阈值见 ANOMALIES.md A 类。
- 对照组灵敏度：A 组 10 基线全 PASS；4 份黄金 reject 夹具全数 REJECT 且码位精确
  （inv2_e2312→E2312,E2322；env_e0002_float→E0002）；已知文档化边界逐份钉住
  （零写者/间接自验证/求值错误不判/E2209 未实装）。

## 4. 异常清单（24 立案 + 5 入档不立案）

全量清单、源码定位（file:line）、逐项复现命令、样本文件（已复制存档 36 份）：
**`anomalies/ANOMALIES.md`**。四句话摘要：

1. **panic（1）**：`nodes: []` → `validate()` 在 `refs.rs:30` BTreeMap 索引 panic，
   违反「validate 返回 Report」契约（应报 E2001）。
2. **crash（4）**：CEL 深嵌套（括号深≥2000 / 90 万段加链，注入点 policies.stop[].when
   等 5 个位点）→ antlr4rust 栈溢出 SIGABRT，`cel.rs` catch_unwind 兜不住；
   阈值 1000 过 / 2000 崩（本机 8MB 栈）。YAML 深嵌套则优雅 E0001（正面证据）。
3. **漏网核心（路径别名族，6 份 PASS）**：节点 id 无字符集限制 + 语义索引按点号
   路径串为键且覆盖无侦测 → 一份文档里「根节点 a.b」冒充「a 的体节点 b」，INV-2
   多写者、E2312、E2321 自验证、E2331 评估输入越权读者、E2322 安全点五条防线
   同时被致盲（ANOMALIES.md B-1 表）。另有 `meta.output.ref` 未登记类型名零违例
   （refs.rs:61 静默短路 + E2205 无核对点）、`has()` 宏逃过 E1052 白名单、
   failures.route/预算 scope 可指进构造体内。
4. **误杀（4 份 REJECT）**：点号 id 节点被 entry/output.from 双口径分裂、
   安全点/from_body 被同名双胞胎误杀、CEL 无法引用点号 id 节点——与漏网同根因，
   修复应一并处理。

## 5. 产物清单

| 路径 | 内容 |
|---|---|
| `farm/` | harness 源码（Cargo.toml + src/main.rs） |
| `gen.py` | 种子化生成器 v1.0（三策略，`python3 gen.py <outdir>` 重生成 500 份） |
| `samples/` | 500 份样本 + 上级 `manifest.jsonl`（逐份 expect/note/strategy） |
| `results.tsv` | 500 份裁定（第二轮 `results_rerun.tsv` 为确定性对照） |
| `anomalies/` | 36 份可疑样本存档（含最小化探针 M001）+ `ANOMALIES.md` 异常清单 |

## 6. 未覆盖面（如实登记）

`af_dsl::registry`（包清单装载，不在 load_str+validate 契约内）；`ValidateOptions`
非默认档（context_quota_cap / declared_graphs 只测默认值）；E900x 部署期码位
（af-compiler 域）；Cargo.lock 未逐行核对（依赖版本以 crate manifest 声明为准：
cel-interpreter "0.10"、serde_yaml 0.9.34）；栈溢出阈值仅本机 8MB 栈单点标定；
YAML 5 万层解析耗时 12-18s 的性能面未做系统测量。
