# RESULT.md — af-harness / 统一 agent 执行器外壳

## ① 结论

**通过**。`cargo test -p af-harness` 30 个单元测试全绿，`cargo clippy --all-targets -p af-harness -- -D warnings` 无新增警告。
程序性是数据：一次 Run 由四元组 [`AgentSpec`](src/spec.rs) 驱动（端点、上下文模板、工具集、策略），不是一段写死的代理代码。

四元组样例（与 `spec.example.json` 同一份字节，`EXAMPLE_SPEC_JSON`）：

```json
{
  "v": 1,
  "body": {
    "endpoint": {
      "protocol": "openai-chat-completions",
      "base_url": "https://api.moonshot.cn/v1",
      "model": "kimi-k2",
      "key_ref": "kimi.prod",
      "reasoning_tier": "low",
      "stream": false
    },
    "context_template": {
      "system": "你是产线执行器。任务：{{task}}\n已有工件：{{artifacts}}",
      "slots": ["artifacts", "task"]
    },
    "toolset": [
      {
        "name": "git",
        "description": "只读查看仓库状态",
        "parameters": {
          "type": "object",
          "properties": { "args": { "type": "string" } },
          "required": ["args"],
          "additionalProperties": false
        },
        "effect": "pure"
      },
      {
        "name": "pay",
        "description": "一次性外部付款",
        "parameters": {
          "type": "object",
          "properties": { "cents": { "type": "integer" } },
          "required": ["cents"],
          "additionalProperties": false
        },
        "effect": "external_once"
      }
    ],
    "policy": {
      "max_turns": 8,
      "stop": {
        "completion_signals": ["DONE"],
        "tool_free_turn_limit": 2
      },
      "budget_ref": {
        "dimension": "llm.cents",
        "per_turn": 1,
        "unit": "cents"
      }
    }
  }
}
```

## ② 做了什么

- **规格**：拒绝式信封 `{"v":1,"body":...}`。缺 `v`、`v` 不是整数、`v` 不是 1、信封有未声明字段，都在解析 body 之前拒绝。body 用 `deny_unknown_fields`。`key_ref` 只存引用名，拒绝 `sk-` 前缀和写进 URL 的用户信息。
- **渲染**：`{{槽}}` 字面替换，不递归，不引入模板库。声明了的槽必须注入；模板里出现未声明槽，在 `validate` 拒绝。
- **端点**：`openai-chat-completions`（bigmodel / kimi / step 形态，路径 `{base}/chat/completions`）和 `anthropic-messages`（路径 `{base}/v1/messages`，头 `x-api-key` 与 `anthropic-version: 2023-06-01`）。默认非流式，`stream=true` 时把 SSE 组装成同一种 `ModelMessage`。明文密钥是 `EndpointKey`，Debug / Display 为 `EndpointKey(***)`，错误文本会抹掉。
- **Run**：每轮渲染 → 调模型 → 解析工具调用。纯函数本地执行。`persist` 与 `external_once` 一律 `declare_intent` + `execute`，载荷只有 `{"arguments","tool"}`，幂等键是工单 id、`{run_id}/{tool}`、`turn*1000+index`。事件写 `af-trace`（`harness.model` / `harness.tool` 走 normal，终态走 critical）。`af-stop` 吃 `ToolOk` / `ToolError`。`af-meter` 每轮先探活再扣；不足则停，类型是 `BudgetFault`。
- **凭证**：生效白名单 = 规格工具名 ∩ 父凭证 `tool(...)` 事实，经 `attenuate` 收紧。判定用 `denial`，一次一个 `request_tool`。不走 `evaluate`（Cedar 只放行 `git` / `test`）。空交集衰减到哨兵 `__af_harness_none__`，白名单保持空。越权是 `RunStatus::Unauthorized`，不是继续跑。
- **落库**：`dir/harness.db`，指纹 `af-harness/run-schema-1`。WAL、`busy_timeout=5000`、`synchronous=NORMAL`、`foreign_keys=ON`，读回核对。空库 `Created`，声明一致 `AlreadyDeclared`，指纹不符或多余对象 fail-closed，不迁移。断点续跑核对规格、变量、工单 id、止损策略、白名单和预算声明；运行列（对话、轮次、止损快照、当前余额）从库里恢复。父凭证和 API 明文不落库。
- **时刻**：`Clock` 注入。网关撤回窗口是 `Duration::ZERO`，执行不靠睡眠。测试 HTTP 是本 crate 的 `stub`（`127.0.0.1`）。真实供应商调用只在 `examples/smoke.rs`，缺环境变量时打印一行并退出。

## ③ 证据

```
$ source /data/af-factory/env.sh && cargo test -j 4 -p af-harness -- --test-threads=8

running 30 tests
test budget::tests::count_probe_stops_when_stock_is_short_and_declared_budget_must_match ... ok
test endpoint::tests::key_debug_and_display_hide_the_secret ... ok
test endpoint::tests::anthropic_json_and_sse_parse_tool_use ... ok
test endpoint::tests::openai_json_and_sse_parse_tool_calls ... ok
test render::tests::missing_slot_value_is_rejected ... ok
test render::tests::replaces_slots_literally_and_does_not_reexpand ... ok
test render::tests::unknown_placeholder_is_rejected_at_spec_validation ... ok
test endpoint::tests::endpoint_error_redacts_the_secret_if_the_body_echoes_it ... ok
test endpoint::tests::stub_anthropic_request_carries_version_and_thinking ... ok
test endpoint::tests::stub_sees_openai_and_anthropic_shapes_without_logging_the_key ... ok
test spec::tests::rejects_version_field_and_unknown_keys_before_body ... ok
test spec::tests::rejects_secret_shaped_key_ref_and_url_userinfo ... ok
test spec::tests::example_envelope_roundtrip ... ok
test stub::tests::parses_content_length_and_lowercases_headers ... ok
test store::tests::file_open_is_created_then_already_declared_with_wal_recipe ... ok
test store::tests::tampered_fingerprint_and_extra_table_fail_closed ... ok
test authz::tests::empty_intersection_does_not_fall_back_to_parent_facts ... ok
test authz::tests::intersection_drops_tools_the_parent_does_not_carry ... ok
test authz::tests::spend_over_cap_is_credential_denied_while_the_fact_remains ... ok
test tests::completion_signal_ends_the_run ... ok
test tests::max_turns_stops_before_another_model_call ... ok
test tests::budget_exhaustion_stops_before_the_next_model_call ... ok
test tests::missing_required_argument_does_not_call_the_handler ... ok
test tests::tool_free_turn_limit_ends_the_run ... ok
test tests::fingerprint_streak_stops_the_run ... ok
test tests::spend_over_credential_cap_is_refused ... ok
test tests::interrupted_side_effect_replays_on_resume ... ok
test tests::pure_tool_stays_local_and_external_once_goes_through_gateway ... ok
test tests::tool_event_is_in_the_normal_trace ... ok
test tests::tool_outside_intersection_is_refused ... ok

test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Doc-tests af_harness: test result: ok. 0 passed; 0 failed
```

```
$ source /data/af-factory/env.sh && cargo clippy -j 4 --all-targets -p af-harness -- -D warnings

    Checking af-harness v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.65s
```

clippy 没有打出 `af-harness` 的 warning。cargo 另有一条依赖包 `proc-macro-error2` 的 future-incompat 提示，不是本 crate 的诊断。

本 crate 原先没有测试。30 个都是这次新增。

## ④ 反馈

1. 判定必须走 `denial`，不能走 `evaluate`。Cedar 策略只放行 `reviewer+test` 和 `builder+git`，盖不住执行器工具名。事实删不掉，所以「事实还在」只是拒绝结果上的观察字段。
2. 空交集不能退回父凭证。衰减块只能加 check，白名单空了就会放行全部父事实。哨兵名 `__af_harness_none__` 只写进衰减块，不写进生效白名单。
3. 续跑时调用方递交的账本必须是首启声明（spent 0 / 初始 meter），当前余额以 `harness.db` 为准。把扣过的账本再递进来会 `DeclarationMismatch`。
4. 完成信号是子串。`DONE` 会命中更长的词。没有为这个边界加正则库。
5. `persist` 和 `external_once` 走同一条网关。分类记在 trace 上，不改变幂等语义。纯函数不登记意图。
6. 提交前中断不写运行行。网关回执已经在，续跑重放，处理函数不会再被调用。

## ⑤ 遗留

- `examples/smoke.rs` 没有在本环境打过真实供应商。缺 `AF_HARNESS_BASE_URL` / `AF_HARNESS_API_KEY` / `AF_HARNESS_MODEL` 时只打印一行并退出 0。
- 同一目录不能同时开两个 `Opened`。trace 落盘线程和 sqlite 连接都绑在这一次打开上。
- 单轮工具调用上限 1000（幂等序号 `turn * 1000 + index`）。超过则本轮返回 `Error::Protocol`，不提交。
- 计数预算（`Meter::try_consume`）有单元测试；Run 级验收用的是美分账本。
- 网关撤回窗口固定为 0，执行器没有暴露 `withdraw_intent`。
- 执行器单线程使用。`Gateway` 的连接不是 `Sync`。
