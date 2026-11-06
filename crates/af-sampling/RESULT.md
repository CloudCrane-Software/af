# RESULT.md — poc-sampling 抽样注入与升迁引擎（crates/af-sampling）

## ① 结论

**通过**。三机制全部实现、可编译、测试真实执行：`cargo build -j 4 -p af-sampling` 通过，`cargo test -j 4 -p af-sampling` **23 个测试全绿（22 单元 + 1 端到端，0 失败）**，零警告。三个指定验证点全部落地：
- CSP-1 1000 件埋缺陷模拟：转抽检/回全检/f_min 托底三不变式逐件核验通过；
- 金标注入拦截契约：告警数恒等于放行缺陷金标数，全检下零告警、抽检下告警如实触发；
- 升迁引擎：判据全部由外部 JSON 配方注入，熔断降级/连净回升/判据晋升在端到端轨迹中四类动作全部真实发生。

## ② 做了什么

crate 结构（约 1350 行，依赖仅 serde+serde_json+thiserror）：

| 文件 | 内容 |
|---|---|
| `src/csp1.rs` | CSP-1 状态机：`Screening`（全检，连续 i 件合格→`Sampling`）/ `Sampling`（以 f 抽检，抽中缺陷→回 `Screening`）。构造期校验拒绝 `f<f_min`，`set_rate` 把调整钳制在 `[f_min,1.0]`（双重结构性保证）；`enabled=false` 整体直通关闭 |
| `src/golden.rs` | 已知答案注入器：`GoldenInjector::enqueue_real` 以率 p 把金标件（70% 缺陷/30% 干净）塞进 `VecDeque` 队列再入真实件；`GoldenLedger::observe` 在放行点对账，**金标缺陷件被放行→`alarms`+1**，误拦干净金标单独计 `false_rejects` |
| `src/promotion.rs` | 升迁引擎：`Lineage`（Agent<Hybrid<Deterministic，只动一级）+ `Recipe`（window/min_successes/max_violations/min_consistency/clean_to_recover，**全部来自外部 JSON**，`Engine::from_recipe_json` 解析并校验）。语义：硬失败→熔断自动降一级（Agent 触底）；连净 N 次→向已达成谱系 `attained` 回升一级（不越级）；滚动窗口积满按三判据裁决晋升并刷新 `attained` |
| `src/rng.rs` | 自带 xorshift64\*，固定种子全程可复现，不引入 rand |
| `tests/end_to_end.rs` | 三机制串联：1400 件（8% 随机缺陷+第 600~624 件 25 连缺陷"事故段"）→注入器→队列→CSP-1→台账；每 100 件折算 RunOutcome（成功=零告警、违规=告警数、一致性=该批金标分类一致率）喂引擎 |

## ③ 证据

```
$ cargo test -j 4 -p af-sampling
running 22 tests ... test result: ok. 22 passed; 0 failed
running 1 test  ... test result: ok. 1 passed; 0 failed

sim1000: planted=92 caught=52 leaked=40 to_sampling=12 to_screening=11 emp_rate=0.205
  （1000 件模拟：缺陷守恒 52+40=92；每件核验①S→Sampling 必有≥15 连检合格
   ②Sampling→S 必为抽中缺陷 ③漏检只发生在未抽中件；经验抽检率 0.205 ≥ f_min=0.05）
contract(f=0.1): alarms=25 intercepted=51 released=25 false_rejects=0 consistency=0.750
  （抽检必然漏检→告警=25=放行缺陷金标；全检对照组 alarms=0、consistency=1.0）
```

端到端 14 批轨迹（固定种子，四类动作全部出现）：

```
batch1: alarms=2 -> Demoted(Agent)        batch9:  alarms=0 -> Promoted(Hybrid)
batch5: alarms=2 -> Demoted(Agent)        batch10: alarms=2 -> Demoted(Agent)
batch2-4/6-8: alarms=0 -> Hold            batch13: alarms=0 -> Recovered(Hybrid)
                                          batch14: alarms=1 -> Demoted(Agent)
final: current=agent attained=hybrid events=14    ledger: alarms=8 (==released), false_rejects=0
```

计数守恒断言全过：缺陷金标 50=拦截 42+放行 8；分批告警加总==台账总告警；`attained ≥ current` 恒成立。

## ④ 对最终方案的反馈

1. **f_min 应做成结构性不变式而非运行期检查**：本 PoC 把"f≥f_min"同时压进构造校验和 setter 钳制两层，测试证明"想压穿 f_min 被顶回"可单点验证。最终方案建议照抄（配置校验拒绝 + 运行期钳制），不要只靠事后审计。
2. **升迁要区分 current 与 attained 两个水位**：熔断只能把 current 打到 attained 之下，连净回升最多回到 attained，冲新谱系必须走判据窗口——否则"连净回升"会变成绕过判据的后门（PoC 初版测试曾暴露此坑）。建议最终接口保留 `attained()` 只读口。
3. **喂给升迁引擎的一致性率要用"该批增量"而非累计值**：累计一致率被早期差批拖死，永远过不了阈值，引擎会卡死在 Agent（端到端初版真实踩中）。批次折算口径（成功/违规/一致性如何从台账增量算出）值得在最终方案里写死。
4. **抽检相位的漏检是机制内生的，不是 bug**：CSP-1 在 f<1 时必然漏检，金标告警的意义就是把内生漏检变成可观测信号。AOQ（平均检出质量）与 f、i 的定量关系本 PoC 未建模，最终方案若要控 AOQ 需要另加一层。
5. serde 化的 `Csp1Config`/`Recipe`/`InjectorConfig` 都是纯数据结构，配方 JSON 直接 `from_str` 即注入，**零内置魔数**，可复用为最终方案的配置面。

## ⑤ 遗留问题

1. clippy 未安装（`'cargo-clippy' is not installed for the toolchain`），静态检查只做到 `cargo build` 零警告，未跑 lint。
2. 抽检决策用自带 xorshift 的伯努利采样，统计断言（如注入率 0.1±10%、70/30 构成）依赖固定种子的确定性，换种子需重调容差；生产化应换可插拔随机源。
3. 升迁判据窗口目前是"积满即评估并清空"的周期制，不是滑动窗口；连续触发节奏受窗口长度影响，最终方案若要更平滑可改滑动窗口。
4. 金标注入只在真实件"之前"入队（保序探针），未模拟"队列拥塞/乱序"场景；队列目前是进程内 `VecDeque`，与真实队列服务的对接未验证。
5. CSP-1 的 i/f/f_min 支持运行期改 f，但不支持运行期改 i（改 i 需重建状态机），语义上是否允许热调需产品拍板。
