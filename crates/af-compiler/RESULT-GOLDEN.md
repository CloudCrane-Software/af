# RESULT-GOLDEN.md — G3 golden 入库（任务包 P2-L07，交付物 D6 地基）

- 工作树：`af-wt-phase2-order`，分支 `phase2-golden`，基线 `2c5a605`
- 构建：`CARGO_TARGET_DIR=~/afbuild-phase2w cargo build -p af-compiler --bin af-compiler-snapshot`（dev profile，7.56s）
- 纪律：只加不改——`tests/golden_test.rs` 为纯追加（`git diff --numstat` = 160 插入 / 0 删除），af-dsl/af-compiler 既有源码与测试语义零改动；4 份 selector 拒绝稿（ADAS / AutoGen-Team-JSON / GPTSwarm / MetaGPT）未入库，等缺组件处置后另行入库；未写 `计划/变更日志.md`。

## 1. 入库清单（17 份 = 11 草稿 + 6 样例）

源 YAML 入 `testdata/src/`，IR 快照入 `testdata/golden/<name>.ir.json`（= CLI stdout 逐字节落盘：pretty JSON + 单一尾换行）。

### 11 份对齐草稿（来源：`计划/研究组/对齐草稿/`，编译干净，未做任何修改）

| 名称 | IR 字节数 | golden sha256（run1 = run2 = 入库文件） | 源 YAML sha256 |
|---|---|---|---|
| ReAct | 27286 | `98d6bbc4b0614c286d7f3d0d7989b7f0f57c0ce3c0891499aee41bb338b6a5b5` | `d13098225435be40c9acd74ff7790a5d75283ccc5dd0cc22042d026aabb1a9d0` |
| Reflexion | 15004 | `91454fd4b55964fc502b7de29355bcebe45a834e26067eefde5ae2de3574f3bd` | `b2062b2094ed70e7c01382118e12fa973ca2fb11018f4ac83ae013cc30ef36a8` |
| Voyager | 22713 | `1720b6f23ff91431c78eeb5b9ba852444ad6b9a373dbdb5c53d695e4a1be64d0` | `1fc0193a03d2ba4af24d68a5d1a5216611d4dad3fb29274ff9c11bbbc8674588` |
| SWE-agent | 27222 | `d2d2355272a5bb4ba0822d510aff1e2a0fe7d3cadd2b16faec7c4ed7111d1a4e` | `fbe7e038827d34c56f87e01fc5df97feed48a15e3e0adada8a01256316411321` |
| MemGPT | 24540 | `07d0b7a8b43bb6476b595799dfe35dcd17c73c22d1523df3b07a6b90ae76b4db` | `c97fa5fbe1c28397e422060cd976c90177308119a8dae0e0a0f4031f36466ac6` |
| CAMEL | 20181 | `e4f8198564f37d979959ca9b856a1378d48d675f3cf4a8b6a4e8cd33702b3f2f` | `90f2a373ed0f05da717620796cc0d48426e65153066daddb4d20219b3b4efd99` |
| Multi-Agent-Debate | 36952 | `149805a38a65f7d72ce417fc846dd899efa1efd08730db31d7915039c5e05667` | `4838cbf11b4bc647cb8aef261027c8716e54b87a3e7ad41728ee3db7995693e0` |
| AFlow | 26195 | `5f2830e0868e5c6b84cf4c46b7ba887b1bf5a750a2c7e999741e66865ceb5bbf` | `e6eae9d3eb05b228f8799439997cbe9e2a37af382e335a71cf0325e45a8e4407` |
| LangGraph-StateGraph | 45353 | `f6662a600b12e5089c8c22cddbe16792b214714a1b92481cb910a03691fbce00` | `d009003e39cfe5a80e505e9e92b11d81cc47fd3c6a7f17fc9ae0a59a699d136e` |
| CrewAI-YAML | 41427 | `ee8531ac470c0ec1d38bfc804c0f37e5547964d3987277668b2f74c83f83b84c` | `9bbacae3b25d9e9f6e82648c82f580f72c7aad5d5204a85c2a1318e53a04afbb` |
| Agensh-DeLM（已修订版） | 45149 | `eac58a10ad8024d937a0ee3c060679e9289e490be7052e58d585c33940ea70d8` | `a20056cca7fbf7a9918b7b1dfd37a7375b6365bad865e7bb9bff572d5d95bced` |

### 6 份 af-dsl 六样例（来源：`af-wt-phase2-dsl/crates/af-dsl/testdata/`，与本仓 `crates/af-dsl/testdata/` 逐字节一致，`diff -q` 六份全过）

| 名称 | IR 字节数 | golden sha256（run1 = run2 = 入库文件） | 源 YAML sha256 |
|---|---|---|---|
| sample1 | 4009 | `0a291ee9bdc3e9c9f7a9a7fae85ec452a49cec4088952df8b0c0a9d9972367dd` | `d6eec9eeb7c67160d06dc8d92668087a23b929dd29ab616a989839f7e2332b1c` |
| sample2 | 11990 | `4640567e8c1189049b1c526203c62c293a666b2cabc5b9d8530756a62a15269a` | `50d80dd2acecfa635ca92d03b4d2e277cd093ce44a45448a830b19f893698ed1` |
| sample3 | 12590 | `e48d528035deeb2ed3086925f69f60406f95f917220eadde79c1c350ee5eb1dc` | `78395fa93102f2e971d79a1f5b291bf975bcd1453085575698560cb304f1dfd7` |
| sample4 | 12675 | `6107a4802e1bf1b5d81e595720339433add354e86aff4192612d1ea6947c6572` | `fc6768f4e36b7bfb1d011fcf06deccac28ed275a3f139bd12fa9a11fec07eba6` |
| sample5 | 10407 | `e41297252e73f420bfa37d8542dfb08866bec6f6221fa4772c82aa7066b0c0a7` | `8c4d31ba8d839a08b189df19fb7166668ab6291a25aa29f408e427de4f456eda` |
| sample6 | 13067 | `f3f1682e65500d0ef79b64da0d9dc8611024d00fa7fdca4823c1add642ff7ec2` | `db903cac8fa1cab91c9ee7d0179786c065e6bc0187529889d24710ce0686fc32` |

## 2. 双跑确定性记录

协议：每份 YAML 用 `af-compiler-snapshot` 独立跑**两次**（各自完整 load → validate 全量零违例 → lower → pretty 序列化），两次 stdout 先 `cmp` 逐位比对，一致才把 run1 落盘为 golden。结果：

- **17/17 双跑 `cmp` 逐位一致，0 份不一致，0 份 CLI 失败**（含 Agensh-DeLM 已修订版——过全量校验，确证编译干净）。
- 每份入库后另做了第三次 CLI 复核：`final CLI↔golden bit-exact: 17/17`。
- golden sha256 与两次运行的 sha256 三者相等（上表 hash 即入库文件实测值）。

## 3. 回归测试（tests/golden_test.rs，纯追加 3 个）

- `g3_golden_corpus_byte_exact`：对 `testdata/src` 每份 YAML 重新 load+validate+lower+serialize（与 CLI 同口径：`snapshot_json_pretty` + `\n`），与 `golden/*.ir.json` **逐字节比对**；不一致时报告期望/实际字节数与首个差异位。
- `g3_golden_corpus_manifest_closed`：`testdata/src` 的 `*.yaml` 与 `testdata/golden` 的 `*.ir.json` 文件集合恰等于 `G3_CORPUS` 闭集清单——未登记文件混入即红。
- `g3_golden_tamper_detection`（篡改检测，证明测试有牙齿）：临时把 `sample1.ir.json` 首字符 `'{'` 改为 `' '` 落盘 → `catch_unwind` 断言逐字节核验**失败** → 恢复原字节 → 复核通过。测试跑完后 `sample1.ir.json` sha256 与 CLI 新鲜输出一致（`0a291ee9bdc3e9c9…`），恢复无损。

## 4. 复现命令

```bash
CARGO_TARGET_DIR=~/afbuild-phase2w cargo build -p af-compiler --bin af-compiler-snapshot

# 复现任意一份 golden（stdout 与 golden 文件逐位一致，含尾换行）
~/afbuild-phase2w/debug/af-compiler-snapshot crates/af-compiler/testdata/src/ReAct.yaml \
  | cmp - crates/af-compiler/testdata/golden/ReAct.ir.json && echo BIT-EXACT

# 全量回归（含 golden 逐字节比对 + 闭集守卫 + 篡改检测）
CARGO_TARGET_DIR=~/afbuild-phase2w cargo test -p af-compiler
```

## 5. C10 变更纪律说明（golden = 宪法 C10 基线）

快照自本 commit 起成为回归基线。此后任何 `testdata/golden/*.ir.json` 的变化只允许两种解释，不存在第三种：

1. **有意识演进**：DSL/lowering 语义有计划变更——先立 ADR，再同步改 golden 与 `G3_CORPUS` 清单登记（闭集守卫强制：绕过登记直接放文件会红）。
2. **回归**：无 ADR 而快照变了——这就是回归信号，修复使其回到基线，而不是改基线迁就。

「为让测试变绿而改 golden」不属于以上任何一种，一律禁止。

## 6. 验收记录

- `CARGO_TARGET_DIR=~/afbuild-phase2w cargo test -p af-compiler`：**26 passed / 0 failed**（既有 23 + 新增 3 全绿；golden_test.rs 单文件 8 passed，含新增 `g3_golden_corpus_byte_exact` / `g3_golden_corpus_manifest_closed` / `g3_golden_tamper_detection`）。
- 磁盘开工检查：`df -h /` 19G 可用（>15G 阈值，放行）。
