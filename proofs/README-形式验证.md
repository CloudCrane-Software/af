# af 形式验证工件库（proofs/）——FV-1 第一波收编

- 收编批：FV 收编批 2026-10-25（裁决书：`计划/研究组/形式化/FV-1-第一波-裁决-20261025.md`）
- 上游定理正本：`计划/研究组/形式化/theorems-v1.md`（57 条义务定稿；本库只收证明工件，不改正本）
- 源码锚定基线：`af-wt-phase2-dsl` @ `phase2-g2b`（commit `e85bfdb`，全部四流只读引用，零改动）
- 目录结构 = wave1 四流原名 + Rocq 陈述库；仓库根 `Cargo.toml` 以 `exclude = ["proofs"]`
  将本目录隔离出 workspace（harness 均为独立 cargo 工程，不参与 workspace 构建）

## 一、三线载体与当前 verified 计数

| 线 | 载体 | 目录 | verified 计数（内核裁决） | 状态 |
|---|---|---|---|---|
| **Kani** | Kani 0.68.0 / CBMC 6.11.0 镜像穷尽 harness | `Kani镜像员-INV组/` | **32**（`Complete - 32 successfully verified harnesses, 0 failures, 32 total`，28.6s；收编批 CI step9 复跑 32/32 同判） | 已验（A-01..A-16 双向） |
| **Kani** | 同上（确定性组+构造语义组） | `确定性组-构造语义-Verus/`（Verus 本体安装失败，本组内核证据全部为 Kani 载体） | **17**（`SUITE SUMMARY: pass=17 fail=0 total=17`；另 3 条承继 FV-0.1 route 12 verified 不重证） | 已验（11 条陈述 + C-05 部分 + 3 条受阻归因如实） |
| **Verus** | Verus 0.2026.10.18 镜像证明（tmpfs 安装，本体不进 CI） | `状态机组-Verus/` | **29 verified, 0 errors**（`verus-state-group.log` 入档完整内核输出，干净复跑同判） | 已验（D-01..D-09 全 9 条） |
| **Rocq** | Coq 8.20.1 陈述库（只译不证：`Proof. Admitted.` = 待证义务登记） | `rocq-规范库-v0/` | **10/10 陈述 coqc 类型检查通过**（21 条定理 Admitted 排队 FV-0.4+ 证明战役） | 陈述定稿，证明待后续批次 |

合计：**Kani 49（32+17）· Verus 29 · Rocq 10/10 陈述**——对应 FV-1 裁决书 §一 四流验收全达成。

定理-实装冲突 2 项（A-03/A-07，源码零改动）与机器反例实证见
`Kani镜像员-INV组/CONFLICTS.md`（conflict-demo harness，feature 门控不入默认集）；
陈述修订全部进 `计划/研究组/形式化/theorems-v0.2-议程.md`。

## 二、复现命令（按线）

### Kani · INV 镜像套件（32 harness，CI step9 即此命令）

```sh
# 前提：Kani 0.68.0 tmpfs 保留件（/dev/shm/fv0/kani-0.68.0 + /dev/shm/fv0/rustup，
# 重启即失；重装配方见 计划/研究组/FV0-工具链PoC-20261024.md §2.1）
cd proofs/Kani镜像员-INV组/harness
env RUSTUP_HOME=/dev/shm/fv0/rustup \
    RUSTUP_TOOLCHAIN=nightly-2026-08-21-x86_64-unknown-linux-gnu \
    PATH=/dev/shm/fv0/kani-0.68.0/bin:$PATH \
    CARGO_TARGET_DIR=~/afbuild-phase2fv-kani \
    cargo kani
# → Complete - 32 successfully verified harnesses, 0 failures, 32 total.（exit 0）
# 冲突实证（预期 FAILURE=证据，feature 门控）：
#   cargo kani --features conflict-demo --harness a03_literal_statement_conflict
#   cargo kani --features conflict-demo --harness a07_literal_statement_conflict
# 注：RUSTUP_TOOLCHAIN 显式覆盖仓内 rust-toolchain.toml 的 1.99.1 钉住——
#     harness 必须跑在 Kani 自带工具链上。
```

### Kani · 确定性组套件（17 harness，不入 CI——c02 单条实测 143.4s 超 advisory 预算）

```sh
# run-suite.sh 锚定原 tmpfs 沙盒 /dev/shm/wave1/kani-proj（重启即失）——先从仓内副本重建：
mkdir -p /dev/shm/wave1/kani-proj/src /dev/shm/wave1/tmp
cp proofs/确定性组-构造语义-Verus/kani-src/lib.rs /dev/shm/wave1/kani-proj/src/
printf '[package]\nname = "wave1-mirrors"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n[features]\nblocked-harnesses = []\n' \
  > /dev/shm/wave1/kani-proj/Cargo.toml
# 运行（脚本内建 PATH/TMPDIR/CARGO_TARGET_DIR；RUSTUP_* 两变量补齐工具链解析）：
env RUSTUP_HOME=/dev/shm/fv0/rustup \
    RUSTUP_TOOLCHAIN=nightly-2026-08-21-x86_64-unknown-linux-gnu \
    bash proofs/确定性组-构造语义-Verus/kani-src/run-suite.sh /tmp/suite.log
# → SUITE-DONE pass=17 fail=0（收编批以本配方自 proofs/ 副本冷重建复跑 17/17，见 RESULT-FV收编）
```

### Verus · 状态机组（29 verified；Verus 本体不装——安装失败史在案：41 次代理下载
curl exit 35，FV-1 裁决书 §一；tmpfs 安装重启即失，重装配方见 RUN-SUMMARY §6）

```sh
# 前提：/dev/shm/dstate/verus-x86-linux/verus（GitHub release/0.2026.10.18.7d82994）
cd proofs/状态机组-Verus
/dev/shm/dstate/verus-x86-linux/verus --crate-type bin state_group_mirror.rs
# → verification results:: 29 verified, 0 errors（exit 0）
```

### Rocq · 陈述库类型检查（CI step9 同款；coqc 走 PATH，本机 8.20.1）

```sh
cd proofs/rocq-规范库-v0
for f in T*.v; do coqc "$f" || exit 1; done   # → 10× exit=0
# Admitted 定理登记为公理（Print Assumptions 可见）——预期形态，非错误。
# 文件名 T01_<主题>.v 用下划线：coqc 8.20.1 拒绝标识符连字符（README §五探测记录）。
```

### 对抗农田（500 样本裁定 + 24 项异常）

```sh
# 重生成 500 份样本（确定性：seed=20261024 同字节；含期望表 manifest.jsonl）
python3 proofs/对抗农田报告/gen.py <outdir>
# 复跑裁定 harness（farm/Cargo.toml 的 af-dsl path 依赖指向历史工作树
# /home/pan-ding/workspace/af-wt-g4/crates/af-dsl——复跑前按需改指当前仓 crates/af-dsl）
cd proofs/对抗农田报告/farm && CARGO_TARGET_DIR=<外置> cargo build --release
# → results.tsv 口径：PASS 198 | REJECT 297 | PANIC-CAUGHT 1 | CRASH-SIGNAL 4
# （农田修复批后复跑：PANIC/CRASH/TIMEOUT 全归零——见 RESULT-农田修复-20261025.md）
```

## 三、收编取舍清单（入 git vs 留路径索引）

| 工件 | 尺寸 | 处置 | 理由 |
|---|---|---|---|
| `kani-full-run.log.gz`（INV 组全量逐 harness 日志） | 168K gz（解压 ~1.7M） | **不入 git**；路径索引：`计划/研究组/形式化/proofs-wave1/Kani镜像员-INV组/logs/kani-full-run.log.gz`（编排者归档处） | 超大压缩日志；仓内已有等价内核证据 `logs/kani-full-run-results.txt`（逐 harness 裁决+耗时，4K）+ RUNLOG.md 全量誊录 + CI step9 可随时复跑再生 |
| 农场 `samples/`（500 份基线样本） | 6M/500 文件 | **不入 git**；可确定性重生成（`gen.py` + seed=20261024，manifest.jsonl 逐份在仓） | 生成器零随机源外泄；manifest 逐份登记 expect/note/strategy，审计不必携全量语料 |
| 农场 `anomalies/*.yaml`（39 份异常样本） | ~1.6M | **不入 proofs/**——已由农田修复批收编为回归语料：`crates/af-dsl/testdata/farm-anomalies/`（39 份 + ANOMALIES.md 副本，表驱动测试 13 个覆盖 39/39） | 语料的工程位置是回归测试而非证明工件；仓内 `anomalies/ANOMALIES.md` 逐项附样本名+复现命令可对号 |
| 农场 `target/`（构建产物） | 192M | **不入 git** | 构建产物，随时可重建 |
| `rocq-规范库-v0/.zcode/`（会话元数据） | — | **不入 git** | 工具会话状态，非交付工件 |
| 其余（全部 kernel 日志 txt/log、harness/镜像/生成器源码、四流报告、十条 .v、manifest/results TSV） | 共 ~2M | **入 git（逐字节原样）** | 收编批已 `diff -r` 逐字节核对与 wave1 交付一致（唯一差 = 上表 .gz 有意排除）；「kernel 日志与 harness 源码入仓」为任务令明文 |

## 四、与 CI 的关系（step9 advisory 门）

`ci/run.sh` step9 为本库的非阻塞早警报门：9a Rocq coqc 10/10 + 9b Kani INV 镜像 32
harness 快检（实测合计 ~39s，预算 <2min；工具缺席只 WARN 不红；确定性组 17 harness
不入快检——c02 单条 143.4s 超预算）。八门硬门语义不变；step9 FAIL/WARN 均不影响
exit code。Verus 本体不装（安装失败史在案），其 29 verified 以入档日志为准。

## 五、边界与残余义务（如实）

- **镜像↔实现对账单测**未建（FV-0 §1.2 建议、四流 RUNLOG/RUN-SUMMARY/对应表均登记）——后续批次义务。
- **Verus 无界归纳版**（确定性组 B-01/B-04/B-06/C-02/C-03 陈述首选工具）：`verus-prepared/b_loop_guard_e2002_e2004.rs` 为已写未跑草稿，未获内核裁决不计 verified——待代理窗口。
- **C-06/C-08/C-09** 受阻归因（kani-compiler ICE / CBMC 病态面）见 `确定性组-构造语义-Verus/C组确定性-对应表.md` §三。
- **A-16 运行时半边** v0 不可立（执行面读权限白名单层占位）——无 harness 义务，非失败。
- 农场 5 项文档化边界入档不立案（ANOMALIES.md B-5/B-6）；24 项立案异常已由农田修复批全数处置（RESULT-农田修复-20261025.md）。
