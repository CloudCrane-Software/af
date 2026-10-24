# af-dsl M3 施工结果（P2-L04）

- 分支：`phase2-dsl-m3`（基 M2 完成点 `c22d994`）。模块边界即审计边界（ADR-007）。
- 范围：①INV-3 生成器-验证器分离——**直接自环判定**（验证面节点任一入边的
  生产者不得是它自己，E2321；§8-4 v0.1 口径：间接自验证不查）；②INV-4 评估器
  外部化——`eval_input: true` 的 store 读者受限扫描（经 `reads[]` 与
  `context.sections` from:store 段引用它的全部节点必须 ⊆ {writer, 绑定
  `uses.unit` 的 check 节点}，E2331）；③提案 §5 inv3/inv4 违规样例逐字节
  提取为 golden 夹具。
- 权威输入：《DSL-语法提案-B》§5 INV-3/INV-4 正式判定规则 + §6 码表
  （E2321/E2331 为已分配码位，未私占空号）+ §8-4（直接自环口径）+ §8-5
  （eval_input 标记责任：误标只收紧不放松）。
- 不变量边界：M4 全部未动工（route 归一化/CEL 静态判定/topology_selector）；
  E2322 未实装（见偏差 1）；既有 crate 与 M1/M2 已交付测试零改动（M1+M2 的
  130 例断言原样全绿）。

## 验收命令与结果（实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| 全量测试 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl` | **152 通过 / 0 失败**（envelope 11 + parse 37 + refs 37 + inv1 36 + inv2 9 + **inv3 10 + inv4 12**） |
| 静态检查 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets` | **0 警告** |
| 红绿序列 | `git log --oneline c22d994..HEAD` | RED `d70ae91`（22 例：11 反例红/11 正例绿，红均因 E2321/E2331 缺席——left 为现状码集）→ GREEN `873744b`（152 全绿），严格先红后绿 |
| 夹具逐字性 | `diff` 提案围栏块（awk 提取）vs `testdata/reject/*` | **零差异**（两夹具均逐字节一致） |

正例门槛：提案 B §4 六个 golden 样例（`testdata/sample1-6.yaml`）全量
（M1+M2+M3）校验零违例——特别是样例 3 Debate（map 并行 + gate 安全点）与
样例 4 AFlow（`answers_ref` eval_input 仅由 check+unit 的 `assess` 读、
`experience` 未标记故 `synth`/`best` 自由读），另有 sample4 专项测试
（`sample4_answers_ref_readers_stay_clean`，断言无 E2331）。

反例覆盖（错误码逐字断言 = 码集 `codes()` 精确相等 + 违例信息逐串比对）：

- **E2321 ×5**：§5 golden 夹具（恰 `[E2321]`，信息逐字对齐 §5 预期错误风格）
  · gate 自环 · human 标记节点自环（验证面 ∪ human 的归属面）· loop 体内
  check 自环（非根作用域，信息含全路径 `l1.verifier`）· 双自环边一次收齐
  （`[E2321, E2321]`）。
- **E2331 ×6**：§5 golden 夹具（恰 `[E2303, E2331]`——§5 注明 gen_a 兼踩
  E2303，两信息逐字）· 非 LLM 候选经 `reads[]`（纯 E2331，无 E2303）· LLM
  候选经 `context.sections` from:store 段（第二条读通道；入边端口已被
  from:edge 段恰一引用故无 E2304）· check 缺 `uses.unit`（非评估器）·
  gate+unit（验证面 ≠ 评估器，§5 严格双条件）· 双读者并犯按 BTree 路径序
  一次收齐。
- 边界正例：间接自验证（check → transform → check 数据流回环）v0.1 不查
  （§8-4，`indirect_self_verification_not_flagged_v01` 钉界）· 非验证面
  节点自环不判（INV-3 判定对象限定验证面）· check 内建 `passed` 端口跨
  节点消费合法 · writer 自读合法 · 未标 eval_input 的 store 完全不查
  （§8-5 单向安全性）· writer 路径悬空时该 store 跳过 INV-4（恰
  `[E2001]`，refs 层已定罪——承 M1 偏差⑨「不重复定罪」纪律）。

## 规则清单（`src/validate/inv3.rs` + `src/validate/inv4.rs`，管道序
inv2 → inv3 → inv4，一次收齐）

1. **E2321 验证面自环**：验证面 = kind ∈ {check, gate}（字面量，开放词表
   内的内建判定面，与 `mod.rs` 内建 `passed` 端口口径一致）∪ `human`
   构造在场。对验证面节点，在其所在体作用域的边表中找 `from == to ==
   本节点 id` 的边，每条报一条；信息含节点全路径与被消费端口
   （`e.input`）。间接自验证（生产者 ≠ 自己但数据流回环）v0.1 不查。
2. **E2331 eval_input 读者受限**：对 `eval_input: true` 的 store 建合法
   读者集 = {声明 writer（点号路径解析后的全路径）} ∪ {kind=check 且
   `uses.unit` 在场的全部节点（全局集）}；读者 = `reads[]` 含该名或
   context 有 `from: store` 且 `store` 指该名的节点（含体内嵌，全路径
   标识）；每个越界读者一条。评估器与 store 的绑定关系（哪个评估器读哪份
   参考答案）不核对——运行期组件装配事项（E9003 域），与 §5「evaluator
   nodes」复数口径一致（样例 4 的 assess 即此面）。

抑制规则（承 RESULT-M1 偏差⑨，避免噪声级联）：writer 路径悬空（refs 层
E2001 已定罪）→ 该 store 整体跳过 INV-4；E2303 与 E2331 对同一 LLM 读者
**同时报**（提案 §5 inv4 样例明确注明兼踩，两码语义域不同不算重复定罪）。

## 错误码分配表（M3 实装 2 码；`src/error.rs` 全 32 码位）

| 码 | name | 触发点 | 层 |
|---|---|---|---|
| E2321 | self_verification | 验证面节点入边生产者 = 自己（直接自环） | 语义·INV-3 |
| E2331 | eval_input_visible | eval_input store 的越界读者（reads/sections 两通道） | 语义·INV-4 |

§6 E23xx 段内未占用余量（E2305-E2310、E2313-E2320、E2322-E2330、E2332+）
未私占；E2322 为 §6 已分配码位但按 P2-L04 切分不属 M3（偏差 1）。

## 与提案 B 的偏差（逐条在案）

1. **E2322（safety_point 非验证面）未实装**：提案 §5 INV-3 段含此规则，
   M2 遗留表曾将 E2321/E2322 并提 M3；但 P2-L04 任务包的 M3 范围条款只列
   「直接自环判定」（验收也只列 E2321 系/E2331 系），且实装 E2322 将使
   M1 已交付夹具断言（`inv2_e2312_map_write.yaml` 恰 `[E2312]`，该夹具
   的 safety_points 是 gate 指向 task 节点）翻红——与「不改 M1/M2 已交付
   测试语义」禁令直接冲突。取任务包范围条款，E2322 留 M4+（实装时需同步
   改 M1 夹具断言为 `[E2312, E2322]` 并在 RESULT 登记）。
2. **E2331 信息用节点全路径**：提案 §5 预期错误风格写 `node 'gen_a'`
   （裸 id），实装为 `node 'run_map.gen_a'`（map 体内全路径）——跨作用域
   消歧所必需，M1 E2312（`map 体节点 'debate_map.debater'`）已确立全路径
   先例。码位与规则名（`eval_input_visible`）逐字对齐。
3. **评估器 = 全局 check+unit 集合**：§5 集合字面「绑定 uses.unit 的
   check 节点」无 store 绑定语义；任何 check+unit 节点读任何 eval_input
   store 均放行。收得更紧的绑定核对（如评估器须消费候选输出）需组件契约
   （E9003 域），v0.1 不做。
4. **严格双条件**：gate+unit、check 缺 unit 均非评估器（各有反例钉住）。
   §5 字面「kind=check + uses.unit」；gate 是验证面（INV-3 域）但评估/
   判定实现的外部化要求专指 check。
5. **INV-3 判定面 = kind 字面 check/gate**：kind 是开放词表（M1 决策），
   check/gate 内建判定与 `passed` 端口同口径；human 标记 = `human` 构造
   在场（与 kind 正交，样例 5 的 human 节点即 kind=task + human）。
6. **非验证面自环不判**：INV-3 判定对象限定验证面；task 节点自环属根图
   裸环（E2005，af-graph 域，本 crate 未实现——RESULT-M1 已登记，不变）。

## 遗留（移交后续模块，均未动工）

- **E2322**：safety_points kind 白名单（§5 INV-3 段；见偏差 1，实装需协调
  M1 夹具断言）。
- M4：route 归一化（E2101-E2103）、CEL 保守子集静态判定（E1051-E1053）、
  topology_selector 解析。
- 构造形状三码（E1005/E1006/E1007，含「unit 仅 check 可用」的 E1005 面——
  本模块 gate+unit 反例在 E1005 落地前依赖 E2331 拒绝）与 E2209/E2210
  归属待裁决（M1 遗留，未变）。
- 间接自验证（check → transform → check）完全判定需数据流可达性分析
  （§8-4 开放问题，提案明示留后续）。
- eval_input 的 writer 辅助启发式（§8-5 开放问题，提案明示待定）。

## 复现

```bash
cd /home/pan-ding/workspace/af-wt-phase2-dsl-m2
CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl        # 152 passed
CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets  # 0 warnings
git log --oneline c22d994..HEAD                                # RED → GREEN
```
