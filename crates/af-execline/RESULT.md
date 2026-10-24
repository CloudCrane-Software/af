# af-execline RESULT — 任务包 P2-F01 订单生命周期 PoC

> 身份：第二阶段执行面实现代理（TDD）｜工作树：`/home/pan-ding/workspace/af-wt-phase2-order`（分支 `phase2-order`，基于 guardrail-fix2）
> 日期：2026-10-24｜依据：ADR-002（crate 划分）/ ADR-003（订单持久化）/ ADR-004（MES 统一）/ ADR-007（TDD）/ ADR-009（资源纪律）

## 1. 做了什么

新建 crate `crates/af-execline`（纯进程内实现，无 docker / LLM / 网络），打通最小订单生命周期：

```
构造订单 → af-engine 门控（bind_line→attach_voucher→release→start，全部真引擎子进程）
        → 执行桩单元（产出工件文件）→ attach_artifact → complete → close
```

全程订单 ledger 落盘（`ledger.jsonl`：append-only、序号连续、整单快照随行、fsync）+ MES 事件构造（af-mes `MesEvent` 类型 + 自有 JSONL stub writer，不连 ClickHouse，ADR-004 过渡态）。这是"统一执行面"的第一块砖：证明订单语义与执行可编程耦合。

| 模块 | 职责 |
|---|---|
| `engine.rs` | `EngineClient`：定位/构建 af-engine 二进制并子进程调用（9-op 单行 JSON stdin/stdout 协议，不打桩、不链接引擎库） |
| `driver.rs` | `OrderLifecycleDriver`：生命周期驱动；幂等键 `{stable_id}/{op}` 重放语义；release/start/complete/close 四类 MES 记账 |
| `ledger.rs` | `OrderLedger`：JSONL 账本（序号由账本盖、落盘 fsync 后才更新内存、重载校验序号连续 + 身份一致） |
| `mes.rs` | `MesFileWriter`：MesEvent JSONL stub（`work_order_id` 收敛到 stable_id） |
| `exec.rs` | `ExecuteUnit` trait + `StubExecuteUnit`（对齐 落地契约-v0 §1 方向，PoC 不求全） |
| `clock.rs` / `error.rs` | 时钟注入（SystemClock/FixedClock）；稳定错误码 EX1xxx（引擎）/ EX2xxx（账本）/ EX3xxx（执行单元），ADR-007 §3 |

git 序列（先红后绿再重构，ADR-007）：

| 提交 | 性质 | 当时测试状态 |
|---|---|---|
| `4821921` test(af-execline): RED 订单生命周期 PoC 验收测试 | 红 | lib 4 failed / 6 passed；integration 8 failed / 0 passed（行为全部 unimplemented!() 桩） |
| `d08ec25` feat(af-execline): 最小实现转绿 | 绿 | lib 10 + integration 8 全过 |
| 见 git log（refactor 笔） | 重构 + 文档 | lib 14 + integration 8 全过 |

## 2. 测试清单（验收标准可执行）

验收命令（任务书原文）：

```
cd /home/pan-ding/workspace/af-wt-phase2-order && CARGO_TARGET_DIR=~/afbuild-phase2 cargo test -p af-execline
```

结果：**全绿**（lib 14 passed + integration 8 passed + doc 0）。另跑 `cargo fmt --check --all` 与 `cargo clippy -p af-execline --all-targets`：零警告。

af-engine 二进制由测试内 `EngineClient::locate` 按需构建（`CARGO_TARGET_DIR=~/afbuild-phase2`，未占用 af/target——ADR-009；实测构建产物 `~/afbuild-phase2/debug/af-engine`）。找不到/构建失败 = EX1001 / EX1002 明确失败，不静默跳过。

集成测试（`tests/lifecycle.rs`，8 例，覆盖任务书四类验收 + 负向）：

1. `happy_path_full_lifecycle_ledger_and_mes`——快乐路径：全生命周期走通（终态 Closed、history 4 条边、工件 1 件）；ledger 可重载且序号 1..N 连续、八个 op 全部有成功记录、成功行均带整单快照；MES 恰含 release/start/complete/close 四类；`work_order_id == stable_id`（ADR-004 收敛）；`stable_id` 与 `af_workorder::StableId::generate` 同规则；判定走真引擎子进程（`engine_bin()` 路径断言）；工件文件落 `out/artifacts/`。
2. `release_without_voucher_is_typed_gate_rejection_not_panic`——门控拒绝：未 attach_voucher 直接 release → 引擎 `PreconditionsNotMet{missing:[BudgetVoucherNotIssued]}`，驱动层转 `ExeclineError::EngineRejected`（EX1006，携带类型化 Missing 清单），不 panic；状态不跃迁；拒绝入账（error_kind=gate、无快照）。
3. `release_with_unsigned_voucher_names_the_voucher`——负向：未签名凭证被点名（`BudgetVoucherNotSigned{voucher_id}`）。
4. `close_without_qualified_final_artifact_is_rejected`——close 拒绝：Completed 无工件 → 引擎拒绝（`NoQualifiedFinalArtifact`），停在 Completed；补齐工件后放行，close 成功记录唯一。
5. `second_drive_replays_without_duplicate_terminal`——幂等重放：同 order_id 二次驱动 → `replayed=true`、终态不变、history 不增、close 成功记录仍唯一、`replay_skip` 审计行在账、序号仍连续、MES 不重复、工件文件不重写。
6. `failed_unit_keeps_order_in_progress_and_retry_completes`——负向：执行单元失败（EX3001）→ 订单停 InProgress、失败入账、幂等键不落；重试放行并走完剩余生命周期。
7. `ledger_seq_gap_is_rejected_as_corrupt`——负向：序号断层、混单身份 → EX2002（不猜不补）。
8. `driver_open_rejects_ledger_of_another_order`——负向：异单账本不得冒充（EX2002）。

库单测（14 例）：时钟（固定/系统）、错误码表稳定连续、Gate 拒绝线解析（含未知变体 fail-loud）、ledger 往返与断层、MesEvent 往返、stub 工件落盘、引擎请求线形状（op/order/参数合并）、状态名线格式、拒绝入账标签。

## 3. 与落地契约-v0 的差距

- **§1 执行单元**：只落进程内 trait + stub。unit.json 声明文件、host_form 注册表、`af.plan.record.v1` 统一结果行、argv 模板/资源下限均未实现（PoC 范围外）。
- **§6 失败恢复**：有幂等键与"单元失败可重试"实证；无 attempt 序号目录（a1/a2 不覆写）、无跨宿主 done_union、无 requeue 退避、无 FAIL_LIMIT 人工终态、无 run_store 恢复。
- **§5 预算**：voucher 只用布尔 + amount 占位（继承 af-workorder 语义）；无 af-meter 执行级账本、无熔断（af-stop）、无计划级对账。
- **approve_decision**（引擎"必经"第三门）未纳入本 PoC 生命周期（任务书 op 清单未含）；决策工件四层校验路径留待后续包。
- **cancel 边**（created/released/in_progress → cancelled）未驱动；终态冻结语义由 af-workorder 侧测试覆盖，本面只走快乐路径 + 两类拒绝。

## 4. 遗留 / 已知边界

- **GateRejection 镜像（重要）**：af-workorder `GateError::MutationForbidden` 的 `op: &'static str` 字段使 `GateError: Deserialize<'de>` 只在 `'de: 'static` 时成立（serde 对 `&str` 字段的隐式借用规则收集 `'static`），**无法从引擎 stdout 的运行时缓冲区反序列化**（已实证：`serde_json::from_str::<GateError>(局部 String)` 编译不过）。客户端用保持线格式同名同形的 `GateRejection` 镜像承接（`op` 落 `String`；未知变体/未知字段 fail-loud，不静默丢错）。af-workorder 若演进 GateError，本镜像需同步——建议后续在 af-workorder 侧提供 owned 形态或改 `op` 为 `String`（改底座语义需新 ADR，不在本包范围）。`DecisionError` 无此问题，直接用 af-workorder 原类型。
- **MES stub 是文件 JSONL**：无 outbox 攒批/重发、无 ClickHouse 写入、无 at-least-once 重试；落盘失败向上传播（不静默吞事件）。目标态由 af-mes `MesWriter` 承接（ADR-004）。
- **传输层错误不入 ledger**：进程拉起失败/非零退出/坏响应（EX1003/1004/1005）发生时 op 未被判定，账本无行；at-least-once 重试会重跑。目标态应入账或走 outbox。
- **0 工件语义**：执行单元成功但产出 0 工件时仍记 `attach_artifact` ok=true（执行完成、无登记），幂等键落账——重试不会重跑该单元；close 门随后因缺合格终态工件拒绝。
- **引擎构建依赖**：测试机需 cargo 工具链；无引擎时不静默跳过（EX1001/EX1002 fail-loud）。
- **独立验证未做**：ADR-007 §4 要求实现/验证分离——本包测试由实现代理按任务书验收标准自写，尚待验证代理独立对拍（含与 orchestration/ Python 演示链的语义对照）。

## 5. 复现方式

```
cd /home/pan-ding/workspace/af-wt-phase2-order
CARGO_TARGET_DIR=~/afbuild-phase2 cargo test -p af-execline   # 全绿
# 产物样例（测试 scratch，/tmp 短命件）：ledger.jsonl 八行序号连续；
# mes-events.jsonl 四类跃迁各一；artifacts/fa-<order_id>.json 工件文件。
```

## 6. CI 七门验证（2026-10-24，工作树 phase2-order，CARGO_TARGET_DIR=~/afbuild-phase2）

| 门 | 结果 | 证据 |
|---|---|---|
| step1 `cargo fmt --check --all` | PASS | 零 diff |
| step2 clippy + 基线门（workspace --all-targets） | PASS | 全 workspace 0 警告；基线 0 签名，无新增 |
| step3 `cargo test -j4 --workspace` | PASS | 687 passed / 0 failed，78 个 target 全 ok（含本 crate lib 14 + integration 8） |
| step4 cue recipe vet+export+结构断言 | PASS | 合并 9 值全中、键集闭合；越界反例被拒 |
| step5 parse_scrub × 7 真实日志 | PASS | 2 正例 exit=0、5 反例 exit=1 |
| step6 cosign 断网 verify-blob | **未跑** | `bin/cosign` 不在工作树（.gitignore 忽略 `/bin`）且本机无该二进制——没跑过就是说没跑过；guardrail-fix2 基线 `ci/out/SUMMARY.txt` 记录该门在当时 PASS，本提交未重新验证 |
| step7 `cargo deny --offline check` | PASS | advisories / bans / licenses / sources 四类全 ok |

资源纪律（ADR-009）：开工 `df` 根分区余量 55G（>15G 阈值）；构建只落 `~/afbuild-phase2`；未占用 af/target；未起/碰任何容器与工厂守护进程。
