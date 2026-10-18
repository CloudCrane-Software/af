# RESULT.md — poc-workorder「订单门控与决策工件」

## ① 结论

**通过**。五点要求全部实现并在本机真实执行验证:`cargo test -j 4 -p af-workorder` 34 个单测 + 3 个集成测试全绿,`cargo run -p af-workorder --example gate_demo` 可跑通并打印门控拒绝原因。

## ② 做了什么

crates/af-workorder(原空壳,已有 Cargo.toml/lib.rs 由引导创建,workspace 根未动),新增 7 个模块:

| 要求 | 实现 | 位置 |
|---|---|---|
| 状态机+门控转换 | `OrderState`/`Transition` 合法边表;`WorkOrder::transition()` 先查状态机边再查门控。release 前置=产线版本已绑定+预算凭证已签发;close 前置=存在 `is_final && qualified` 工件;非法转换报 `IllegalTransition`,门控不满足报 `PreconditionsNotMet{ missing: Vec<Missing> }`(类型化缺失清单,不是字符串) | `src/state.rs`、`src/order.rs:189-233`、`src/error.rs` |
| SLA 字段 | `Sla{deadline: Option<String>(RFC3339), priority: Priority(Low<Normal<High<Critical>)}` + `is_overdue()` | `src/sla.rs` |
| graph_version 不可变绑定 | `GraphVersion` 私有内字段;`WorkOrder.graph_version` 私有字段**只有读取器,无任何 setter/mut 通路**;换版走变更单生成后继工单(`supersedes`),原单封存 | `src/order.rs:17, 116` |
| 变更单数据结构 | `ChangeOrder{diff_summary, diff_paths, from/to_graph_version, drain, relay}`;`DrainStrategy{NoDrain/DrainAllInflight/DrainWithCap}` + `RelayStrategy{ColdHandover/WarmHandover/HotTakeover}`;`validate()` 拒 from=to、空差异描述、全量排水×热接力矛盾组合;`applies_to()` 防陈旧变更单 | `src/change.rs` |
| 决策工件 | `DecisionArtifact{actor_id, role, permissions:[Read/Approve/Revise/Arbitrate], decision, payload_hash, signature}`;四层校验:hash 一致→权限不越角色上限→decision 有对应权限→占位签名非空。只读角色签发 approve 在第③层拒绝;伪造塞权限在第②层拦截 | `src/decision.rs:135-172` |

占位诚实声明:payload_hash 用 FNV-1a64(`src/hash.rs`,无抗碰撞,跨进程确定);signature 为 ed25519 占位结构(算法标签+key_id+字节,只验非空,不做真密码学)。依赖仅 serde/serde_json/thiserror,未加 tokio/rusqlite/sha2。

## ③ 证据

命令:`cargo test -j 4 -p af-workorder`(cwd=af workspace 根,rustc 1.99.1):

```text
running 34 tests
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured
     Running tests/lifecycle.rs
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

命令:`cargo run -j 4 -p af-workorder --example gate_demo`(错误文本即"给出缺什么"的证据):

```text
[release 未绑产线/未签凭证] → 门控拒绝 Release:前置条件缺失 [LineVersionNotBound, BudgetVoucherNotIssued]
[凭证 bv-9 未签名]          → 门控拒绝 Release:前置条件缺失 [BudgetVoucherNotSigned { voucher_id: "bv-9" }]
[created 跳步 complete]      → 非法状态转换 Created → Completed:状态机不存在这条边(允许的出边:[Release, Cancel])
[close 无合格终态工件]       → 门控拒绝 Close:前置条件缺失 [NoQualifiedFinalArtifact]
close   ✅ → Closed
履历:[Created→Released, Released→InProgress, InProgress→Completed, Completed→Closed]
```

关键断言(测试真实执行,非编译即过):`order::tests::close_requires_final_and_qualified_artifact` 逐级验证无工件/终态不合格/合格非终态三次拒绝后才放行;`decision::tests::reader_issuing_approve_is_rejected` 断言 `PermissionDenied{required: Approve, granted: [Read]}`;`graph_version_never_changes_through_any_operation` 走完全程后断言版本仍为 `g-3`。

## ④ 对最终方案的反馈

1. **门控错误建议定为"类型化缺失清单"**:`PreconditionsNotMet{ transition, missing: Vec<Missing> }` 让上层能程序化区分"缺产线绑定"和"缺预算凭证",LLM 编排层可直接把 `Missing` 翻译成"下一步该找谁补什么"。字符串错误会丢掉这个能力。
2. **非法转换和门控不满足要分成两种错误**:先查状态机边、再查门控,否则"created 直接 close"会被误报成"缺工件",语义混乱。建议最终方案保留这个两层结构。
3. **graph_version 不可变与"变更"天然冲突,解法是后继工单**:不可变绑定 + 变更单 spawn 后继单(`supersedes`)+ 原单封存,比"可变版本号+变更日志"更符合审计语义。代价是后继单要重走 release 门控(凭证/绑定不继承白给)——这我认为是特性不是缺陷,但需要产品拍板确认。
4. **排/接策略是正交字段,但要查组合矛盾**:DrainAllInflight(排空才切)与 HotTakeover(在制立即接管)互斥,`validate()` 里拦了。最终方案若策略变多,建议把合法组合矩阵显式列出。
5. **坑:测试里"在 InProgress 问 close 门控"拿到的是 IllegalTransition 而非缺失清单**(close 门控只存在于 Completed→Closed 这条边上)。编排层若想"提前预检 close 前置",需要一个独立的 `check_close_readiness()` 只读接口,而不是真调 `close()`——最终方案建议补这个 dry-run API。
6. **workspace 依赖管理**:根 Cargo.toml 无 `[workspace.dependencies]` 且不许动,依赖版本只能写在自己 crate 里;11 个 crate 各自加 serde 时版本可能漂移,建议引导阶段统一放 `[workspace.dependencies]`。

## ⑤ 遗留问题

- **ed25519 真签名**:`Signature` 结构已对齐(算法标签/key_id/64 字节),但只做占位校验;最终方案接 ed25519-dalek 或复用 af-credential 的签名能力(可能该 crate 会做,建议 af-workorder 只依赖其 trait)。
- **payload_hash 是 FNV-1a64**:跨进程确定但无抗碰撞,必须换 SHA-256/BLAKE3;换时 `DecisionArtifact` 接口不变,只动 `hash.rs`。
- **deadline 是字符串比较**:`is_overdue()` 只对同格式 RFC3339 UTC 成立;最终方案接时间类型(jiff/chrono)或统一由 af-calendar 出时间。
- **Clock 未抽象**:履历无时间戳,SLA 超时判定靠外部传入 `now`;真实实现应注入 `dyn Clock` 以便测试确定性。
- **预算凭证只验 `signed` 布尔位**:未验签发方、未与 af-credential/预算系统联动;金额也只登记不校验。
- **权限模型是静态角色→权限映射**:没有"同一单上审批人不能是被审批人"这类冲突规则;多决策工件汇聚(如"需 2 个 approve 才放行")只在集成测试里演示了单票场景,法定人数(quorum)逻辑未实现。
