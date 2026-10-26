# FV-0.2 Kani PoC 运行证据（2026-10-24）

环境：Kani 0.68.0（standalone bundle `kani-0.68.0-x86_64-unknown-linux-gnu.tar.gz`，
140,315,343 B，解压 485M，全部装于 `/dev/shm/fv0` tmpfs——不占磁盘）；
工具链 nightly-2026-08-21（rustc 1.100.0-nightly 8925ea358）；CBMC 6.11.0；
16 核 / 14G RAM 主机。仓库：af-wt-phase2-execline @ phase2-fv0。

## ✅ 通过的 harness（`cargo kani`，默认集）

```
Checking harness smoke_u8...                                      VERIFICATION:- SUCCESSFUL  0.016s
Checking harness mirror_branch_exclusion_exactly_one_all_valuations... VERIFICATION:- SUCCESSFUL  0.058s
Checking harness mirror_normalize_all_gotos_exhaustive...         VERIFICATION:- SUCCESSFUL  0.196s
Complete - 3 successfully verified harnesses, 0 failures, 3 total.
SUMMARY: ** 0 of 118 failed (4 unreachable)
```

穷尽范围声明：
- `mirror_normalize_all_gotos_exhaustive`：goto 分配 3×3×3=**27 全枚举**
  （rules[0]/rules[1]/default ∈ {t0, t1, exit}）；
- `mirror_branch_exclusion_exactly_one_all_valuations`：when 赋值 2×2=**4 全枚举**；
- `smoke_u8`：u8 全域。

## ❌ 受阻路线（为什么默认集是镜像而不是真实实现）

以下探针均在 `/dev/shm/fv0/probe`（与 proofs/kani 同样的 af-dsl path 依赖）实测，
每条注明命令形态与超时。

### §C/§E — `validate` 链：kani-compiler 0.68.0 ICE（硬阻断）

- §C：直接构造最小 `Document`（2 节点/1 store/合法写者）调 `af_dsl::validate`
  → `cargo kani --harness probe_c_direct_document_validate`
  → **internal compiler error: kani-compiler/src/intrinsics.rs:243:17
  assertion failed: matches!(output.kind(), RigidTy::Int(I32))**（编译期，未到 CBMC）。
- §E：**空文档**（0 节点/0 store）调 `validate` → 同一 ICE。
  ⇒ ICE 与 harness 内容无关，在 validate 的静态可达链上
  （cel-interpreter → chrono 的某 intrinsic 实例化；envelope::check 路径
  不经过该链，可编译）。
- 推论：**任何经 `af_dsl::validate` 的 INV-2 harness 在 0.68.0 下无法编译**。
  受阻 harness 全文保留于 `src/pending.rs`
  （`--features blocked-harnesses` 启用），ICE 修复后即为正式入口。

### §G/§F — 真实 `route::normalize`：CBMC 运行时不可接受

- §F：`probe_f_normalize_real_impl`（1 规则 × 2 目标，goto/default 符号二选一，
  4 用例）：编译通过，`timeout 420` 未完成（CBMC 常驻 ~95% CPU）。
- §G：**纯具体单用例**（无任何 kani::any）：`timeout 300` 未完成。
  ⇒ 瓶颈在实现本体：BTreeSet/BTreeMap 操作 + `format!` 守卫串构造
  （route.rs:151-168）在 CBMC 的堆/字节模型下代价爆炸。
- 对比：同算法的 Vec/整数镜像（lib.rs `mirror_normalize`）27 用例 0.196s。

### §A — 信封路径：serde_yaml::Value 构造本身病态

- `probe_a_construct_only`：**只构造** `Mapping` + 插入一个 version 键值，
  不调任何 af-dsl 代码：`timeout 240` 未完成。
  ⇒ serde_yaml::Value/IndexMap（hashbrown）在 CBMC 下不可用。
  4 个信封 harness（编译通过）一并留在 pending.rs。

### 版本对照尝试

- Kani 0.66.0 bundle 下载在当日 20:10-20:25 窗口因代理 SSL 失败
  （curl exit 35，重试 5 次 + 60s 退避一轮）未完成——0.66 对照未跑成，
  如实登记。代理：http://100.64.0.8:7890。

## 复现

```sh
export RUSTUP_HOME=/dev/shm/fv0/rustup
export RUSTUP_TOOLCHAIN=nightly-2026-08-21-x86_64-unknown-linux-gnu
export PATH=$RUSTUP_HOME/toolchains/$RUSTUP_TOOLCHAIN/bin:/dev/shm/fv0/kani-0.68.0/bin:/usr/bin:/bin
export LD_LIBRARY_PATH=$RUSTUP_HOME/toolchains/$RUSTUP_TOOLCHAIN/lib
export CARGO_TARGET_DIR=/dev/shm/fv0/kt2
cd proofs/kani && cargo kani          # 3 harness 全绿
```

（tmpfs 重启即失；kani bundle 与工具链的获取命令见报告 §FV-0.2。）
