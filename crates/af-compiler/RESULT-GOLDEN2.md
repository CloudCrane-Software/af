# RESULT-GOLDEN2.md — G3 golden 第二批入库（任务包 P2-L10，基线 17 → 21）

- 工作树：`af-wt-phase2-order`，分支 `phase2-golden2`，基线 `8fed107`（含 P2-L09 registry + packages + fix2）
- 构建：`CARGO_TARGET_DIR=~/afbuild-phase2w cargo build -p af-compiler --bin af-compiler-snapshot`
- 磁盘开工检查：`df -h /` 246G 可用（45% 用量，充裕）
- 纪律：
  - 4 份草稿从 `计划/研究组/对齐草稿/` 复制入 `testdata/src/`，入库前后 `cmp` 逐字节同（零修改）；草稿正本未动。
  - 17 份既有 golden 与既有测试语义零改动——17 份 golden sha256 实测与 RESULT-GOLDEN.md 记录逐一相等（见 §1 表，非转抄）。
  - `tests/golden_test.rs`：`git diff --numstat` = **148 插入 / 2 删除**，2 删除即任务特许的「清单闭集守卫改 21」两行（`g3_golden_corpus_manifest_closed` 的 doc 行与 `let manifest` 行改两清单并集），其余全部为文件末追加。
  - 未写 `计划/变更日志.md`。

## 1. 入库清单（21 份 = 17 基线 + 4 selector 稿）

源 YAML 在 `testdata/src/`，IR 快照在 `testdata/golden/<name>.ir.json`。17 份基线走无注入链（`af-compiler-snapshot <yaml>`），4 份 selector 稿走注入链（`af-compiler-snapshot --registry af/packages <yaml>`）；golden 均为 CLI stdout 逐字节落盘（pretty JSON + 单一尾换行）。

### 17 份基线（首批入库，本次仅复核未改动；sha256 实测 = RESULT-GOLDEN.md 记录）

| 名称 | IR 字节数 | golden sha256 | 源 YAML sha256 |
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
| Agensh-DeLM | 45149 | `eac58a10ad8024d937a0ee3c060679e9289e490be7052e58d585c33940ea70d8` | `a20056cca7fbf7a9918b7b1dfd37a7375b6365bad865e7bb9bff572d5d95bced` |
| sample1 | 4009 | `0a291ee9bdc3e9c9f7a9a7fae85ec452a49cec4088952df8b0c0a9d9972367dd` | `d6eec9eeb7c67160d06dc8d92668087a23b929dd29ab616a989839f7e2332b1c` |
| sample2 | 11990 | `4640567e8c1189049b1c526203c62c293a666b2cabc5b9d8530756a62a15269a` | `50d80dd2acecfa635ca92d03b4d2e277cd093ce44a45448a830b19f893698ed1` |
| sample3 | 12590 | `e48d528035deeb2ed3086925f69f60406f95f917220eadde79c1c350ee5eb1dc` | `78395fa93102f2e971d79a1f5b291bf975bcd1453085575698560cb304f1dfd7` |
| sample4 | 12675 | `6107a4802e1bf1b5d81e595720339433add354e86aff4192612d1ea6947c6572` | `fc6768f4e36b7bfb1d011fcf06deccac28ed275a3f139bd12fa9a11fec07eba6` |
| sample5 | 10407 | `e41297252e73f420bfa37d8542dfb08866bec6f6221fa4772c82aa7066b0c0a7` | `8c4d31ba8d839a08b189df19fb7166668ab6291a25aa29f408e427de4f456eda` |
| sample6 | 13067 | `f3f1682e65500d0ef79b64da0d9dc8611024d00fa7fdca4823c1add642ff7ec2` | `db903cac8fa1cab91c9ee7d0179786c065e6bc0187529889d24710ce0686fc32` |

### 4 份 selector 转绿稿（本次入库，registry 注入链；来源 `计划/研究组/对齐草稿/`，零修改）

| 名称 | IR 字节数 | golden sha256（run1 = run2 = 入库文件） | 源 YAML sha256 | 服务包（图数） |
|---|---|---|---|---|
| ADAS | 36527 | `2c2c2db86ca38b76ed988466fceafe5a16b5e768622aeef0b142bb967419e804` | `a878e8cb27960b6142f4559b14df88445d8599739500a7a2a5be06f7f48341e2` | adaskit（3） |
| AutoGen-Team-JSON | 43797 | `2ff775d15dba27a9f36c3005ed13dc85b6c7d8cc1b290da240af0b28a535bc6f` | `630c79bd43695256fe820ced460e32c710efb8acfb701eddaba2e4edf58fe5fa` | autogen（5） |
| GPTSwarm | 32017 | `72b7aedcfe7f4bb1f70763d37510a68783e6ae62dc809c6f3c1c3b31b0074065` | `1e92e954df221ad3405fb29176cd2b193f005b7f5d37b4704746b4a8d6aa3816` | swarmkit（3） |
| MetaGPT | 35523 | `3d75b1eda353be72260d816409388452b80dfdee6c8c63a95f0d6610b3b56e74` | `0ed267255924cc01272520fe99470a744dfe39ea94b399c5b4fbe738cab7c521` | sweng（3） |

## 2. registry 注入链（做法登记）

4 份稿的 `topology_selector` 候选图引用（`pkg.<包>.<名>@<版本>`）只在包图清单声明（裁决 §二-1：候选图是包产物，单文档不自带声明面），故 validate 必须经注入才过 M4 候选核对（E2001）。全链：

```
af-dsl::registry::Registry::load(af/packages)      # 扫 4 包 manifest.json → declared_graphs（14 引用）
  → reg.options()                                  # ValidateOptions { declared_graphs, ..Default }
  → af_dsl::validate_with(&doc, &opts)             # 注入后全量校验零违例
  → af_compiler::snapshot_json_pretty(&doc)        # lower + pretty 序列化
  → writeln!（stdout，尾换行）                     # golden = 该输出逐字节落盘
```

- **CLI 未支持注入 → 最小扩展**（任务特许两条路中的前者）：`af-compiler-snapshot [--registry <包根>] <yaml>`，`--registry` 给出时按上链装载注入；不给时与扩展前**逐字节不变**（`validate` 默认空集）。CLI diff = 42 插入 / 6 删除（参数解析 + validate 分支）。
- 扩展两面性实测（同一二进制）：
  - 不带旗标跑 ADAS → **E2001 三条全拒**（rules[0]/rules[1]/fallback 候选图未声明，exit=1）——fail-closed 无隐式放行；
  - 不带旗标跑 ReAct → 与既有 golden `cmp` 逐位一致——17 份基线口径零漂移（21 份全量复核见 §4）。
- 4 份稿的全部候选引用与 `af/packages` 四清单逐一闭合（ADAS→adaskit 3、AutoGen→autogen 5、GPTSwarm→swarmkit 3、MetaGPT→sweng 3，rules + fallback 共 14 引用无缺）；其余 `pkg.*`（tools/memory/template_ref/unit）非 selector 候选，不进注入面。

## 3. 双跑确定性记录

协议承首批（RESULT-GOLDEN.md §2）：每份用扩展后 CLI 独立跑**两次**（各自完整 registry 装载→注入→validate 全量零违例→lower→pretty 序列化），两次 stdout 先 `cmp` 逐位比对，一致才把 run1 落盘为 golden。命令形如：

```bash
~/afbuild-phase2w/debug/af-compiler-snapshot --registry af/packages crates/af-compiler/testdata/src/ADAS.yaml > run1
```

- **4/4 双跑 `cmp` 逐位一致**（ADAS 36527 / AutoGen-Team-JSON 43797 / GPTSwarm 32017 / MetaGPT 35523 字节），0 份不一致，0 份 CLI 失败。
- 入库后每份第三次 CLI 复核：`final CLI↔golden bit-exact: 4/4`（§1 表 hash 即入库文件实测值）。
- 17 份基线在扩展后二进制上全量重跑无旗标链复核：`17/17 bit-exact`——扩展对既有口径零影响。

## 4. 回归测试（tests/golden_test.rs：追加 3 个 + 特许改 1 处）

- `g3p2_selector_injection_is_load_bearing`（注入是承重墙，禁旁路）：对 4 份各断言——默认不注入必含 E2001 拒（`ErrorCode::DanglingReference` 精确断言）；`Registry::load(仓库正本 af/packages)` 恰 4 包；注入后全量校验零违例。
- `g3p2_selector_golden_byte_exact`：4 份沿注入全链（与 CLI `--registry` 同口径）重新 lower+serialize，与 golden **逐字节比对**（含尾换行），不一致报字节数+首差异位。
- `g3p2_selector_tamper_detection`（篡改检测抽样第 2 份，走注入链）：篡改 `GPTSwarm.ir.json` 首字符 `'{'`→`' '` → `catch_unwind` 断言注入链核验**失败** → 恢复 → 复核通过。与既有 `g3_golden_tamper_detection`（sample1，无注入链）合计对 21 份抽样 2 份、覆盖两条核验链，均有牙齿。
- 特许修改：`g3_golden_corpus_manifest_closed` 闭集从 `G3_CORPUS`（17）改为 `G3_CORPUS ∪ G3_SELECTOR_CORPUS`（**21**），src 与 golden 两目录文件集合仍一份不多不少。**RED 记录**：4 份文件先入库而清单未改时该守卫即红（实测 panic：testdata/src 与清单不一致）——守卫有牙齿，非形式品。

## 5. 验收记录

- `CARGO_TARGET_DIR=~/afbuild-phase2w cargo test -p af-compiler`：**29 passed / 0 failed**（golden_test.rs 11 = 既有 8 + 新增 3；总计 26 既有 + 3 新增），golden 21 份逐字节全绿。
- `CARGO_TARGET_DIR=~/afbuild-phase2w cargo test -p af-dsl`：全绿无回归（含 registry_test 20 例；两包合跑 `cargo test -p af-compiler -p af-dsl` 23 个 suite 全 ok，FAILED 零命中）。
- `CARGO_TARGET_DIR=~/afbuild-phase2w cargo clippy -p af-compiler -p af-dsl --all-targets`：**零新警告**——唯一警告 `doc list item without indentation`（golden_test.rs:255 起 `+` 行惰性续行）经 `git stash` 对基线 8fed107 复跑证实**本就存在**，非本次引入；新增码中途曾出 `while_let_loop` 一条，已当场改 `while let` 消除。

## 6. 复现命令

```bash
CARGO_TARGET_DIR=~/afbuild-phase2w cargo build -p af-compiler --bin af-compiler-snapshot

# 复现 4 份新 golden（stdout 与 golden 逐位一致，含尾换行）
~/afbuild-phase2w/debug/af-compiler-snapshot --registry af/packages \
  crates/af-compiler/testdata/src/ADAS.yaml \
  | cmp - crates/af-compiler/testdata/golden/ADAS.ir.json && echo BIT-EXACT

# 复现任一基线 golden（无旗标，口径与首批相同）
~/afbuild-phase2w/debug/af-compiler-snapshot crates/af-compiler/testdata/src/ReAct.yaml \
  | cmp - crates/af-compiler/testdata/golden/ReAct.ir.json && echo BIT-EXACT

# 全量回归（21 份逐字节 + 闭集 21 + 注入承重墙 + 篡改抽样 2）
CARGO_TARGET_DIR=~/afbuild-phase2w cargo test -p af-compiler
```
