# RESULT — poc-release-gate 投放闸与渐放

## ① 结论

**通过**。CONWIP 语义全链路可编译、可跑、15 个测试真实执行全绿(`cargo test -j 4 -p af-release-gate` 退出码 0):按标准审阅分钟计量的 WIP 上限硬闸(阻塞/拒绝两态)、完成释容、下限断粮预警(边沿触发事件)、夜间窗口确定性比例放行、首件渐放(探针封顶→谓词晋升)全部实现;262 件手工到达序列的一天模拟验证了不超限(max_wip=400.0=上限,551 次 WipCap 拒绝)且不断粮(全程 0 个清零 tick,下限预警正确触发并恢复)。

## ② 做了什么

代码结构(`crates/af-release-gate/`):
- `src/gate.rs` — 核心闸门 `ReleaseGate`:
  - `Item{review_minutes}` 工时权重;WIP = 队列内在制权重之和;`GateConfig`/`NightWindow` 全参数注入且支持 serde JSON 反序列化;
  - `try_admit` 策略评估顺序固定:**canary → 夜间窗口 → WIP 上限**;上限判定 `wip + weight > upper`(精确等号可入);
  - `admit_blocking`(经 `SharedGate` = Mutex+Condvar 包装):WipCap 时阻塞,`complete` 释放容量并 notify_all 唤醒;支持超时;
  - 下限:WIP 跌破下限发 `NeedRefill`(边沿触发一次),回升发 `RefillSatisfied`;
  - 夜间窗口:跨午夜区间(`start>end` 表示 wrap);确定性信用制——初始信用 1.0,每请求 +ratio,信用 ≥1 放行并扣 1,窗口进出时重置,长期放行率精确 = ratio;
  - `begin_canary(config, N, predicate)`:gated 态下该配置累计最多放 N 件探针,`complete` 累积 `EvalReport{released, completed, defects}` 喂注入谓词,通过即晋升全量放行(`CanaryPromoted` 事件)。
- `src/sim.rs` — 一天 1440 tick 确定性模拟:逐小时注入产能(白天 5、夜间 1.5 标准审阅分钟/分)、昨日结转在制起跑、FIFO 倾注产能完成出闸、夜间限流件扣暂存出窗归还、rollout 新配置直入渐放泊位;产出 `SimReport`(逐 tick 记录 + 拒绝分类 + `zero_wip_ticks`/`starved_despite_supply` 等指标)。
- `tests/gate_unit.rs`(14 测试)— 上限拒绝/释容/精确等号、超大件永拒、配置校验、下限预警边沿、wrap 窗口包含性、信用比例精确值(50 请求×0.4→21 件)、窗口进出重置、渐放封顶/干净晋升、缺陷持续 gated、阻塞唤醒与超时、JSON 配置注入。
- `tests/day_sim.rs`(1 测试)— 手工构造 262 件到达序列(夜 52/早高峰 24/rollout 突发 8 v2+13 v1/午间 50/空窗 105min/脉冲 20/稳态 ~66/夜 30),断言不超限、不清零、无"有供给却断粮"、下限预警触发、夜间比例带内、渐放 3 探针封顶+晋升、账目守恒。

## ③ 证据

命令与输出(`cd af && cargo test -j 4 -p af-release-gate`):

```
test result: ok. 1 passed  ... (tests/day_sim.rs)
test result: ok. 14 passed ... (tests/gate_unit.rs)     ← 退出码 0
--- day_sim 关键输出:
items=262 total_work=5901min capacity=5520min util=106.9%
admits=242 completions=244 rejects={wip_cap:551, night:56, canary:71}
max_wip=400.0 zero_wip_ticks=0 below_lower_ticks=25 starved_despite_supply=0
night: arrivals=82 admitted=26 ratio=0.317      ← 配置 0.3,带内(±初始信用)
canary: promoted=true promoted_at=568 probes_released_while_gated=3
events: NightWindowExit@360, CanaryPromoted@568, NeedRefill@965(wip113<120),
        RefillSatisfied@987, NeedRefill@988, RefillSatisfied@990, ... NightWindowEnter@1320
--- 供给空窗段(无到达,产能 5/分):
t=920 wip=342 → t=950 wip=199 → t=970 wip=98(<下限120,预警@965)
→ t=980 wip=90(最低点,>0 不断粮)→ t=990 wip=133(脉冲补给恢复)
--- 逐小时剖面(摘):06:00 wip=379(清晨限流补量灌满) 08:00 parked=1 09:00 parked=5
(渐放封顶扣住 5 件 v2,其余流量 wip≈393 照常) 15:00 backlog=3 23:00 deferred=11(留给次日)
```

单元测试证据(节选):`wip_cap_blocks_admit_and_complete_releases_capacity`(20+15>30 拒,完成 20 后同件准入);`night_ratio_is_deterministic_credit_scheme`(50×0.4 → 精确 21 件);`blocking_admit_wakes_on_completion`(阻塞 150ms 仍未准入,complete 后唤醒,wip=10+20=30)。

## ④ 对最终方案的反馈

1. **下限的语义应是"预警线"而非"准入线"**:下限不强制准入,只发 `NeedRefill` 信号,由上游(补量调度)响应。模拟证明它是清零前的有效前兆(提前 ~25 tick 预警)。最终方案建议事件化(NeedRefill/RefillSatisfied 边沿对)而非轮询。
2. **夜间限流需要"逐小时产能"配套**:产能恒定时,夜间只放 30% 必然结构性断粮(实测半途就清零)。真实运营夜间人力少,产能按小时注入后两者才自洽。最终方案里夜间窗口参数应与排班产能一起配置。
3. **限流件不要重试请求**:被夜间限流拒绝的件若原地重试,每次重试都烧信用/虚增请求数。模拟采用"扣到暂存、出窗归还"——建议闸门接口直接提供 `defer` 语义或由调用方约定。
4. **夜间信用在 WipCap 处会被空烧**:请求过了限流闸但被 WIP 上限挡下时,信用已扣——该件重试会再要一次额度。轻微偏差(全天 26/82=0.317 vs 0.3),若要严格,建议先查 WIP 再扣信用,或 WipCap 拒绝时退还。
5. **渐放泊位(新配置专用通道)是必要的**:若 rollout 件混入 FIFO backlog,会被早高峰积压淹没数小时(实测晋升被推到 11:46),渐放就失去意义。最终方案应给新配置独立 intake 通道,代价是泊位优先造成短暂的 FIFO 逆序(PoC 已注明)。
6. **评估报告是累计口径**(`probes_completed`/`defects` 累计):首件带缺陷+严格零缺陷谓词 = 永不晋升。真实系统应按"最近 K 件"滑窗评估——谓词已注入,报告结构可能要加 `recent` 字段。
7. WIP 按整件权重计(经典 CONWIP,完成才释容);若想提高管道度,可改按剩余工时计,但"完成一件释放容量"的语义会变,需拍板。
8. 依赖注意:thiserror 2 拉入 proc-macro-error2(cargo 报 future-incompat 警告,当前可用);依赖都写在本 crate Cargo.toml,未动根 `[workspace.dependencies]`(根 Cargo.toml 按纪律未碰),统一版本时需补登记。

## ⑤ 遗留问题

- 策略评估顺序(canary→夜间→WIP)当前硬编码,若最终方案要可配,需定义优先级语义(如夜间限流是否豁免探针件)。
- `SharedGate` 的 `admit_blocking` 以固定 `now` 重试,长时间阻塞下夜间窗口切换不会被感知(PoC 场景分钟级等待,不影响;长等待需传入时钟源)。
- 夜间限流的暂存归还发生在"出窗后的第一个 tick",与清晨到达合流后顺序为"暂存件在前",未做按优先级/按权重的插队策略。
- 模拟为单审阅池 FIFO 倾注,没有建模审阅人异质性和中断;渐放评估的"缺陷"是手工注入的字段,没有真实的质检信号源。
- 未做 property-based 测试(如任意到达序列下不超限的不变量);当前是 1 条确定性全天序列 + 14 条单元不变量。
