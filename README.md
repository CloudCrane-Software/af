# af — Agent 工厂底座（内核 Rust 工作区）

> **本仓库已迁入 agent黑灯工厂 工作区**（../AGENT.md 是所有 agent 的入口契约）：计划与全网状态在 ../计划/ 与 ../状态/，开发前必读。

> 这是 Agent 工厂底座的内核种子仓库：11 个 crate 是 2026-10-19 凌晨由 15 项 PoC（全部真实执行、约 170 个测试全绿）产出的**已验证原型**——每个 crate 的语义结论记录在其 `RESULT.md`，正式开发从这里出发。
> 设计与实施方案见：`../产品设计_拍板版.md`（根本法）与 `../Agent工厂底座_实施方案_v1.md`（实施方案/波次/启动清单）。

## 快速开始

```bash
source ~/.cargo/env        # rustc/cargo 1.99.1（已装，见 BOOTSTRAP.md）
cargo build -j 6 && cargo test -j 6   # 全量构建与测试（当前全绿）
```

第三方单二进制（temporal/zot/cosign/cue/crane，均已就位于 `bin/`，**不入 git**）的版本与获取方式见 `BOOTSTRAP.md`。

## 布局

```
af/
├── Cargo.toml            # workspace（members=["crates/*"]）
├── crates/               # 11 个内核件原型（Rust）
│   ├── af-credential/    # 凭证签发与权限求交（biscuit-auth 6 + cedar-policy 4；8 测试）
│   ├── af-graph/         # 图模板 schema 与编译期校验（16 测试 + afg validate CLI）
│   ├── af-workorder/     # 订单门控/决策工件/变更单（34 单测 + 3 集成）
│   ├── af-effect-gateway/# 副作用网关：意图/幂等/fencing（6 测试）
│   ├── af-sampling/      # CSP 抽样 + 金标注入 + 三谱系升迁（23 测试）
│   ├── af-release-gate/  # CONWIP 投放闸 + 渐放（15 测试）
│   ├── af-meter/         # 可组合供给计量器（15 测试）
│   ├── af-calendar/      # 通用日历与 SLA 计时（18 测试）
│   ├── af-trace/         # 分级背压 + 缺口计数（14 测试，10 万事件洪峰验证）
│   ├── af-stop/          # 失败指纹止损（12 测试）
│   └── af-station/       # 人工任务账本（8 测试，含并发竞争）
├── poc/                  # 五项外部 PoC（各带 RESULT.md；VERIFY*.md = 诚实清单补验报告）
│   ├── poc-cas/          # zot+crane+cosign 实测（含 GC 实测与离线验签；VERIFY.md=周期GC长跑/scrub/http.compat，VERIFY-TRUSTED-ROOT.md=离线tlog审计）
│   ├── poc-temporal/     # Temporal 语义验证（含 kill -9 恢复与 Rust SDK smoke；VERIFY.md=worker重投递/patched/Worker Versioning）
│   ├── poc-cue/          # CUE 产线定义与配方可行性（VERIFY-FFI.md=cuengine 运行时求值）
│   ├── poc-dspy/         # DSPy IR 结构验证（36 断言）
│   ├── poc-dbos/         # DBOS 对照实测（裁决维持 Temporal）
│   └── POC_SUMMARY.md    # 15 项汇总 + §四补验轮（9/9 清零）
└── bin/                  # 第三方二进制（.gitignore，见 BOOTSTRAP.md）
```

## 开发规约

1. **先读对应 crate 的 RESULT.md 再动它**——里面是已验证的语义结论与坑（例如 af-sampling 的双水位升迁、af-graph 的校验顺序、biscuit 6 无 Float 预算用美分整数）。
2. cargo 一律 `-j 4~6`（本机 16 核 14G 内存）。
3. 零警告纪律；新依赖先过"全网搜索确认无现成实现"与四级漏斗（见拍板版 §2.4）。
4. 波次与任务清单见实施方案 §六；**不设时间估计**。

## 下一步（波次 1 首批任务）

- [ ] 根 Cargo.toml 收编 `[workspace.dependencies]`（防 11 crate 版本漂移）+ 装 clippy
- [ ] 预算数值全链统一整数最小单位（u64 计数 / i64 美分），af-credential × af-meter 词表对齐
- [ ] af-workorder：接入 af-calendar（Sla.deadline → DateTime<Utc>）、FNV-1a64 占位换 SHA-256/BLAKE3、ed25519 签名落地（从 af-credential 下沉）
- [ ] 计时换单调钟（af-effect-gateway SystemTime 可回拨）
- [ ] CUE 预编译 CI 化（cue vet + cue export → JSON，运行时零 CUE 依赖）+ CEL（cel-interpreter）内嵌谓词求值
