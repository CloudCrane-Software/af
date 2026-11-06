# RESULT.md — poc-stop / 失败指纹止损（crates/af-stop）

## ① 结论

**通过**。三条规则全部实现且测试真实执行通过（11 个单元测试 + 1 个 doctest 全绿），
包括三种触发、不误触（不同工具交替错误不触发指纹规则）、规则优先级与指纹库覆盖项。

## ② 做了什么

实现 `crates/af-stop/src/lib.rs`（零外部依赖，std only），纯事件流引擎：

- **接口**：`StopEngine::feed(Event) -> Decision`，`Decision = Continue | Stop{reason} | Escalate{reason}`。
- **事件**：`ToolError{tool, error_code}` / `ToolOk{tool}` / `CheckFailed{node}` / `GenericError{source, error_code}`。
- **参数注入**：`StopPolicy { fingerprint_streak_limit: N=3, normal_error_threshold: M=5, store: FingerprintStore }`，`StopEngine::new(policy)` 注入。
- **指纹库结构预留**：`FingerprintStore`（`HashMap<Fingerprint, FingerprintEntry>`），可按指纹覆盖连击阈值（`Some(0)`=白名单）与触发动作（`StopNoInfluence` 默认 / `Escalate`）。默认空库时行为与"无指纹库"完全一致。
- **规则与优先级**（同一事件命中多条只返回最高一条）：
  1. 连续 N 次相同指纹（工具名+错误码）的工具错误 → `Stop(NoInfluence)`；任何其他事件（成功、不同指纹、check、普通错误）都打断"连续"。
  2. check 型确定性节点首败 → `Stop(CheckFirstFail)`（优先级最高）。
  3. 工具错误+普通错误累计 ≥ M → `Escalate(NormalErrorThreshold)`。
  4. 指纹命中可被指纹库改判为 `Escalate(FingerprintPolicy)`（预留路径）。
- 引擎不锁存停机态：收到 Stop 后调用方应停止喂数据（已写入 doc 注释）。

## ③ 证据

```
$ source ~/.cargo/env && cargo test -p af-stop -j 4   # 在 af/（workspace 根）执行

running 11 tests
test tests::rule1_fingerprint_streak_stops_at_nth ... ok        # ①连续3次相同指纹→Stop(NoInfluence)
test tests::rule2_check_first_failure_stops_immediately ... ok  # ②check首败→Stop(CheckFirstFail)
test tests::rule3_generic_errors_escalate_at_threshold ... ok   # ③累计≥M→Escalate
test tests::rule1_alternating_tools_never_triggers ... ok       # 不误触：search/browser交替20次全Continue
test tests::rule1_success_breaks_the_streak ... ok              # 成功打断连续，重新计数
test tests::rule1_different_error_code_is_different_fingerprint ... ok
test tests::rule3_tool_errors_also_count_toward_threshold ... ok
test tests::fingerprint_stop_wins_over_threshold_escalate ... ok # N=3,M=3 同时命中→Stop优先
test tests::store_override_whitelisted_fingerprint_never_stops ... ok
test tests::store_override_lower_limit_and_escalate_action ... ok
test tests::mixed_stream_end_to_end ... ok
test result: ok. 11 passed; 0 failed

Doc-tests af_stop: test result: ok. 1 passed   # crate 级文档示例也可编译可运行
```

首轮运行曾 4 个测试失败——均为测试场景自身数错步数/忘调高 M（引擎行为正确），
修正测试后全绿；引擎实现自始未改。

## ④ 对最终方案的反馈

1. **接口形态**：`feed(event) -> Decision` 单向拉取式很好用，无回调、无生命周期耦合，
   建议最终方案保持"调用方喂数据、拿 Decision 自行处置"；引擎**不做**停机锁存，
   Stop 后是否继续喂由编排层决定——这个语义边界要写进最终接口文档。
2. **"连续"的定义是关键坑**：打断条件（成功/不同指纹/任何其他事件都算打断）必须显式约定，
   否则同一规则会出现两种实现。本 PoC 采用"指纹级严格连续"，并配了测试钉死。
3. **阈值语义**：`达到阈值`按"第 M 个错误事件当拍返回 Escalate"（`count >= M` 在递增后判断），
   不是"超过 M 才升级"。定规范时建议用本句原话，避免歧义。
4. **优先级建议固化**：check 首败 > 指纹连击 Stop > 普通阈值 Escalate（更具体的规则胜出），
   已有测试 `fingerprint_stop_wins_over_threshold_escalate` 钉死。
5. **指纹库**：预留结构已可用（按指纹覆盖 N、改判动作、白名单）。最终方案若要落库，
   建议登记项就是 `FingerprintEntry{streak_limit, action}` 的形状，加载端（配置/DB）另接。

## ⑤ 遗留问题

- `Stop` 后锁存（幂等返回 Stop）未实现——留给编排层还是引擎层，需最终方案拍板。
- 指纹归一化只做了精确匹配（工具名+错误码字符串相等）；错误码别名/通配（如 `5*`）未做。
- 未接入 serde：`Event`/`Decision` 目前无序列化边界，跨进程传输时需补 derive（零依赖是 PoC 取舍）。
- 无滑动窗口/时间维度（如同指纹 5 分钟内 N 次）；当前只有"严格连续"一种计数语义。
