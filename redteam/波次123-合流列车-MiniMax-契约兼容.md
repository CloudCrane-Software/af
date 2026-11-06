# 波次1-3 合流列车 · 红队审查（API/序列化契约方向）

审查面：serde 线格式向后兼容、af-engine JSON 契约防呆、orchestration/ 与引擎字段漂移、deny.toml 例外范围。
基线 2e06908 → 当前 main（103 个提交）。只读审查，未改任何被审文件（唯一写入是本报告与 /tmp 下的复现脚本）。

## 我实际跑过的检查

| 检查 | 命令 | 结果 |
|---|---|---|
| af-engine + af-workorder 测试 | `cargo test --offline -p af-engine -p af-workorder` | 21（engine 集成）+ 48+3+4+3+3（workorder）全绿，与 RESULT 声明一致 |
| 引擎线协议实测 | 直接 `echo '<req>' \| ./target/debug/af-engine`（十余组载荷） | 见下各条 |
| 供应链门 | `cargo deny --offline check` | `advisories ok, bans ok, licenses ok, sources ok`，exit 0，但有 warning[duplicate] |
| advisory-db 新鲜度 | `git -C <db> log -1` + `grep -rl <ID>` | 见 §4.4 |
| Python/Rust stable_id 一致性 | `python3 -c 'from identity import stable_id_for…'` | **一致**（唯一一条我跑出来是正面的跨语言检查） |
| fuzz 目标是否真能构建 | `cargo metadata --manifest-path fuzz/Cargo.toml --no-deps` | 少一个 bin，见 §5.4 |

没跑的：Temporal dev server 上的 W3E 集成复跑（需要 :7240 服务与 worker 常驻，本轮未起）、`cargo deny` 的联网刷新路径（无网络）。凡涉及这两项的结论我按"未实测"标注。

---

## 1. 编排壳的轨迹/错误分支按错误的字段名读引擎响应（两条恒为 null / 一条恒为 None）

**where** `orchestration/workflow.py:56-63`、`orchestration/workflow.py:109-115`

**问题** `engine_call` 写轨迹时读 `resp.get("state")` 和 `resp["error"]["detail"]["PreconditionsNotMet"]["missing"]`，但引擎响应的状态在 `resp["order"]["state"]`、缺失清单在 `resp["error"]["gate"]["PreconditionsNotMet"]["missing"]`。三个键（`tag`、`transition`、`state_after`、`missing`）在真实响应里全都不存在，于是轨迹 12 行全是 null，`missing` 永远抓不到门控清单。`_engine` 的错误分支读 `err.get("message")`，而 `message` 只有 `BadRequest` 才有；`Gate`/`Decision` 拒绝时拼出来的消息是字面的 `engine rejected release: None`。

**证据**（复现脚本 /tmp/drift.py，逐行照抄 workflow.py:56-63 与 109-115 的取字段逻辑，打真实 `release` 响应）：

```
$ python3 /tmp/drift.py
release raw: {"ok": false, "error": {"kind": "Gate", "gate": {"PreconditionsNotMet": {"transition": "Release", "missing": ["LineVersionNotBound", "BudgetVoucherNotIssued"]}}}, "engine_version": "0.1.0"}
trace_like: {"tag": null, "op": "release", "transition": null, "ok": false, "engine_version": "0.1.0", "state_after": null, "missing": null}
ApplicationError message -> 'engine rejected release: None'  type=Gate
```

作者自己那轮集成实测留下的轨迹文件 `data/engine_calls.log`（未入库，本地残留，12 行）与上表逐字一致：

```
$ wc -l data/engine_calls.log && head -2 data/engine_calls.log
12 data/engine_calls.log
{"t": "2026-10-19T19:08:18.797", "tag": null, "op": "bind_line", "transition": null, "ok": true, "engine_version": "0.1.0", "state_after": null, "missing": null}
{"t": "2026-10-19T19:08:18.827", "tag": null, "op": "attach_voucher", …, "state_after": null, "missing": null}
```

严重性放大点：`poc/poc-bridge/REPORT.md:134` 写「E3-A 的事件历史未逐 run 归档：证据以引擎轨迹（`data/engine_calls.log`）+ workflow list 终态为准」。这份被当成审计证据的轨迹现在既不记状态也不记缺失清单。

**evidence**
- `orchestration/workflow.py:59-62`（`req.get("tag")`/`req.get("transition")`/`resp.get("state")`/`error.detail.PreconditionsNotMet.missing`）
- `orchestration/workflow.py:112`（`err.get("message")`）
- `crates/af-engine/src/lib.rs:29-37`（Response 只有 ok/order/error/engine_version，无顶层 state）
- 实测输出如上；`data/engine_calls.log` 为作者本轮集成跑留下的 12 行。

---

## 2. `approve_decision` 不与订单绑定；编排壳从不调用它

**where** `crates/af-engine/src/lib.rs:188-194`、`orchestration/workflow.py:149-151`

**问题** `approve_decision` 分支把 `order` 完全不读，只把调用方在请求里自带的 `payload` 交给 `artifact.validate_against(&payload)`。因此一份对任意 payload 签发的决策工件，可以对任意工单判定通过——引擎没有任何地方用 `WorkOrder::decision_payload`（`crates/af-workorder/src/order.rs:242`）从订单推出 payload，线协议里也没有这样的 op。而 `README.md:63/65` 与 `RESULT-UPDATE.md:61` 的说法是「编排层要自己先调 `approve_decision`，再调 `release`」，编排壳实际做的审批门是一条裸 signal：

```python
if not s.approved_by:
    await temporalio.workflow.wait_condition(lambda: bool(s.approved_by))
```
`client.py:30` 也只发一个人名字符串，没有角色、没有权限、没有 decision artifact。所以从 signal 到 release 之间，决策与订单之间没有任何一处产生过绑定关系，README 的两步说法在代码里没有对应物。

**证据** 同一份 artifact + 同一 payload，打两张完全不同的订单：

```
$ python3 - <<'EOF'   # payload=sha256 自算，signature=占位 64×0xde
wo-1        -> {"ok": true, "order": {"id": "wo-1", "stable_id": "sid-f905c3b…", …}}
wo-attacker -> {"ok": true, "order": {"id": "wo-attacker", "stable_id": "sid-deadbeef", …}}
```

**evidence**
- `crates/af-engine/src/lib.rs:188-194`：`Op::ApproveDecision` 只 `parse_field(obj,"artifact")` / `parse_field(obj,"payload")`，`order` 未参与
- `crates/af-engine/README.md:63`、`crates/af-engine/RESULT-UPDATE.md:61`
- `orchestration/workflow.py:149-151`（裸 signal 审批门）、`orchestration/client.py:30`（signal 只带一个人名）
- 实测：同一 artifact 对 `wo-1` 与 `wo-attacker` 均 `ok:true`

---

## 3. 未知字段被静默接受并在往返中抹掉；编排壳用这个响应覆盖 workflow 状态

**where** `crates/af-workorder/src/order.rs:91-104`、`orchestration/workflow.py:115`

**问题** `WorkOrderWire` 没有 `deny_unknown_fields`。于是判定是不对称的：**缺**身份字段严格拒绝（拒绝式信封），**多**出来的字段一律静默吞掉。而 `README.md:21` 声称「订单快照整单进、整单出」——只在字段集完全对齐时成立。编排壳第 115 行 `self._s.order = resp["order"]` 直接用这个有损响应覆盖 workflow 状态，于是任何引擎不认识的字段在第一次成功调用后从订单历史里永久消失。这正是版本错配场景下最不该静默的那一类数据（例如将来 `WorkOrder` 加了 `risk_level` 而某个生产者先写了它：引擎不报错，放行判定照跑，只是那个字段没了）。

**证据**

```
$ echo '{"op":"bind_line","order":{…"budget_voucher_v2":{"voucher_id":"bv-9","signed":true},"line_version_pin":"lv-8"},"line":{…}}' | ./target/debug/af-engine
ok= True
v2 field survived round-trip: False
out order: {"id": "wo-1", …, "budget_voucher": null, …}     # 两个注入字段都没了
```

README:21 已经诚实写了「派生实现不拒绝未知字段」，但没有写这个有损往返的后果，也没有写编排壳会拿它覆盖状态。

**evidence**
- `crates/af-workorder/src/order.rs:91-104`（`WorkOrderWire` 无 `deny_unknown_fields`）
- `crates/af-engine/README.md:3,21`
- `orchestration/workflow.py:115`
- 实测：`bind_line` 成功，注入的 `budget_voucher_v2` / `line_version_pin` 未出现在响应 order 中

---

## 4. 拒绝式信封在线上被压成 BadRequest 自由文本，编排侧无法程序化区分

**where** `crates/af-engine/src/lib.rs:145-150`、`crates/af-engine/README.md:45`

**问题** `RejectionEnvelope` 在 af-workorder 里是结构化的 `kind`/`field`/`detail`，经 `serde::de::Error::custom` 走 `WorkOrder::Deserialize` 后被渲染成一句人话，再被引擎原样塞进 `{"kind":"BadRequest","message":"…"}`。三个结构化字段在线上全部丢失，「老载荷缺身份」与「op 字段类型写错」在调用方眼里是同一个 kind。编排壳对两者都抛 `non_retryable=True` 的 ApplicationError，于是：**升级到这版引擎之前创建、还在飞的老快照订单会被永久卡死**，而壳里没有任何迁移路径（`identity.rs` 遗留也只写「没有稳定身份注册表，调用方自己保存」）。

**证据**

```
$ echo '{"op":"release","order":{"id":"wo-legacy","run_id":"","state":"Created",…}}' | ./target/debug/af-engine ; echo rc=$?
{"ok":false,"error":{"kind":"BadRequest","message":"订单快照无法反序列化: 拒绝式信封:老载荷缺少字段 stable_id,拒绝补默认值"},"engine_version":"0.1.0"}
rc=0
$ echo '{"op":"release","order":{"id":"wo-legacy","stable_id":"sid-x",…}}' | ./target/debug/af-engine   # 缺 run_id
{"ok":false,"error":{"kind":"BadRequest","message":"订单快照无法反序列化: 拒绝式信封:老载荷缺少字段 run_id,拒绝补默认值"},"engine_version":"0.1.0"}
```

拒绝行为本身是对的（我确认它确实拒绝、退出 0），丢的是结构。

**evidence**
- `crates/af-engine/src/lib.rs:145-150`：`bad(format!("订单快照无法反序列化: {e}"))`
- `crates/af-workorder/src/order.rs:140-145`、`crates/af-workorder/src/error.rs:143-178`
- `crates/af-engine/README.md:45`（承认归为 BadRequest）
- `orchestration/workflow.py:111-114`（统一 non_retryable）

---

## 5. `with_deadline` 是死开关：SLA deadline 对 release 判定毫无影响

**where** `orchestration/workflow.py:72`、`orchestration/workflow.py:130`

**问题** 字段注释写「E3 构造用：新前置条件（release 需 SLA deadline）下谁会被拒」，`client.py:25` 也把它做成开关。但合流后的 `af-workorder` 里 `Missing` 只有四个变体（`order.rs:361-374` 的 `release_missing` 只查产线绑定与预算凭证），E3 那个 `SlaDeadlineNotSet` 门控并没有随 PoC 一起进来——`poc/poc-bridge/REPORT.md:17` 里它是引擎 v2 的构造条件。结果是 `with_deadline=False` 的单子照样 release 通过，这个开关现在什么也不构造。

**证据**

```
$ python3 -   # 同一张已绑线、已签券的订单，只改 sla.deadline
deadline=None -> {"ok": true, …, "state": "Released", …, "sla": {"deadline": null, …}}
deadline set -> {"ok": true, …, "state": "Released", …, "sla": {"deadline": "2026-12-31T00:00:00Z", …}}
```

**evidence**
- `orchestration/workflow.py:72,130`
- `crates/af-workorder/src/order.rs:361-374`（`release_missing` 不读 sla）
- `crates/af-engine/README.md:87`（引擎明说不判定 SLA）
- 实测如上（两种 deadline 均 `ok:true` → `Released`）

---

## 6. run_id 层端到端失活：没有任何线协议入口能写它

**where** `crates/af-workorder/src/identity.rs:3-5`、`orchestration/workflow.py:127`

**问题** `identity.rs` 的整段论证建立���「Temporal continue-as-new 会更换引擎 run_id，订单身份不能跟着 run 走」。但线协议里没有任何 op 涉及 run_id（`grep -rn "run_id\|RunContext\|continue_as_new" crates/af-engine/src/` 无输出），`WorkOrder::continue_as_new` 的调用方只有测试（`crates/af-workorder/tests/identity.rs:22,64,165`），编排壳在 `workflow.py:127` 写一次 `"run_id": ""` 之后，跨 CAN 再也不更新（`status` query 里那个 `run_id` 来自 Temporal info，从不回写订单）。于是在真实链路上 run_id 恒为空串，`stable_id` 要解决的问题在线上不可观测、也不可验证——W3E 提交信息里「wo-live-3 CAN 续接 runs_done=1 履历跨段连续」这条集成证据（我本轮未复跑）证明的是履历连续，证明不了身份与 run 的解耦。

**evidence**
- `grep -rn "run_id\|RunContext\|continue_as_new" crates/af-engine/src/` → 无输出
- `grep -rn "run_id" orchestration/*.py` → 只有 `client.py:28`、`workflow.py:127`（写 ""）、`workflow.py:198`（只读 Temporal info）
- `grep -rn "continue_as_new" --include=*.rs crates/` → 只有 `crates/af-workorder/tests/identity.rs` 与 src 定义

---

## 7. `identity.py` 复制了 Rust 的派生规则，没有任何跨语言钉死

**where** `orchestration/identity.py:5-8`、`crates/af-workorder/src/identity.rs:18,31-36`

**问题** stable_id 的派生规则在 Rust（`STABLE_ID_DOMAIN` + `StableId::generate`）和 Python（`_DOMAIN` + `stable_id_for`）各写一遍，两侧都由人工保持一致，仓库里没有任何测试或 CI 把它们钉在一起（`orchestration/` 下无测试文件；Rust 侧向量只断言自己）。任何一侧改域前缀或哈希算法，另一侧不会响——同一业务单会按 `client.py:24` 传入的 order_id 在两侧算出两个不同身份，而 `stable_id` 正是用来跨 run 认同一张单的。

**证据** 本轮实测**当前是一致的**（这条我给正面结论，不是缺陷）：

```
$ python3 -c "import sys;sys.path.insert(0,'orchestration');from identity import stable_id_for;print(stable_id_for('wo-1'))"
sid-f905c3b464315a0a05fe2682f64a93aa2194016459b62a5b554f8d60692f451f
   rust 侧钉死向量（crates/af-workorder/src/identity.rs:182）完全相同
```

缺陷在于「靠两条注释保持同步」，不在于当前值。

**evidence**
- `orchestration/identity.py:1-8`
- `crates/af-workorder/src/identity.rs:18,31-36,182`
- 实测输出一致；`find orchestration -name "*test*"` 无结果

---

## 8. deny.toml：advisory-db 刷新脚本的同步段是死代码，门禁数据新鲜度无人保证

**where** `ci/refresh-advisory-db.sh:6-19`、`deny.toml:5-6`、`ci/run.sh:168`

**问题** 脚本 for 循环的两个分支都以 `exit 0` 结束（第 8 行 pull 成功、第 10 行 clone 成功），第 15-19 行的「同步到 cargo 默认库位」永远到不了。就算到了也写错地方：cargo-deny 0.20.2 用的是哈希目录 `~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`，脚本写的 `~/.cargo/advisory-dbs/advisory-db` 根本不存在。而 `ci/run.sh` 的 step7 只跑 `cargo deny --offline check`，不先调刷新脚本。于是 deny.toml 第 5-6 行承诺的「refresh 脚本同步…检查一律离线」这条链路实际不存在，门禁读到的 db 是谁上次联网时留下的快照，本轮它停在 `999ec89 Sat Oct 17 15:19:28 2026`。

**证据**（用假 HOME + 假 git 复跑脚本，证明同步段不可达且 `~/.cargo/advisory-dbs` 从未被创建）：

```
$ HOME=$T/fakehome PATH="$T/stub:$PATH" bash $T/ci/refresh-advisory-db.sh; find $T/fakehome
script exit=0
--- git calls ---
stub-git -C advisory-db pull --ff-only
/fakehome            # 空 —— 第 16-18 行的 DST 从未创建

$ ls ~/.cargo/advisory-dbs/
advisory-db-3157b0e258782691   db.lock        # 只有哈希目录，没有脚本写的 advisory-db
$ git -C ~/.cargo/advisory-dbs/advisory-db-3157b0e258782691 log -1 --format='%H %ad'
999ec89d… Sat Oct 17 15:19:28 2026
```

**evidence**
- `ci/refresh-advisory-db.sh:8,10`（两处 `exit 0`）与 `:15-19`（不可达的同步段）
- `deny.toml:5-6`、`ci/run.sh:166-168`
- 复现脚本输出如上；`ls ~/.cargo/advisory-dbs/` 与 db HEAD 均为实测

---

## 9. deny.toml 的 ignore 是按 advisory ID 全局静音，读起来却像绑定了依赖链

**where** `deny.toml:7-12`

**问题** 两条 `ignore` 的 reason 都描述了具体链路（「经 biscuit-auth 6 传入」「经 cel-interpreter 0.10 传入」），但 cargo-deny 的 `ignore` 按 ID 全图生效，不绑链路。我核对了两条链路今天确实成立（`cargo tree -i` 输出见下），但这个「绑定」只存在于注释里：将来 af-credential 摘掉 biscuit-auth、或有人直接依赖 `paste`，同一条 ID 依然被静默，且没有任何机制提醒复审。

顺带一个事实核对：这两条在本地 db 里都是 `informational = "unmaintained"`、`patched = []`，而 cargo-deny advisories v2 对 unmaintained 的默认档位本来就是 warn——所以 `ignore` 实际只是把 warn 变静默（deny.toml 第 4 行自己也把 `yanked` 设成 warn）。危害不在安全等级，在这三条例外缺乏任何到期/复审约束。

**证据**

```
$ cargo tree --offline -i paste
paste v1.0.15 (proc-macro) └── cel-interpreter v0.10.0 └── af-graph v0.1.0
$ cargo tree --offline -i proc-macro-error2
proc-macro-error2 v2.0.1 └── biscuit-quote v0.3.0 └── biscuit-auth v6.0.0 └── af-credential v0.1.0
$ head -6 ~/.cargo/advisory-dbs/advisory-db-3157b0e258782691/crates/proc-macro-error2/RUSTSEC-2026-0173.md
[advisory] id="RUSTSEC-2026-0173" package="proc-macro-error2" informational="unmaintained"
[versions] patched = []
$ grep -n 'biscuit-auth' crates/af-credential/Cargo.toml
9:biscuit-auth = "6"     # 插入符区间，任何 6.x
```

**evidence**
- `deny.toml:7-12`、`crates/af-credential/Cargo.toml:9`
- 两条本地 advisory 记录（`informational="unmaintained"`、`patched=[]`）
- `cargo tree -i` 两条链路实测

---

## 10. deny.toml 的 `wildcards = "allow"` 范围比注释宽

**where** `deny.toml:30-31`

**问题** 注释把放行范围限定为「monorepo 内 path 依赖……内核 crate 间 path 依赖是设计形态」。但 cargo-deny 的 `wildcards` 档位覆盖的是版本通配（`foo = "*"`）与 git 通配，与 path 依赖无关（path 依赖是 `[sources]` 的另一类）。也就是说这条配置顺带放行了任何未来写进 Cargo.toml 的 `*` 版本要求，注释与实际范围不符。

**证据**

```
$ grep -rn '"\*"' crates/*/Cargo.toml Cargo.toml     # 当前无通配依赖，所以现在无害
（无输出）
$ sed -n '28,32p' deny.toml
[bans]
multiple-versions = "warn"
wildcards = "allow"
```

**evidence**
- `deny.toml:30-31`
- `grep -rn '"\*"' crates/*/Cargo.toml Cargo.toml` 无输出（当前无命中，故为潜在范围过宽，非现网缺陷）

---

## 11. deny.toml 注释给的命令形态是错的

**where** `deny.toml:6`

**问题** 注释写「检查一律 `cargo deny check --offline`」，但 cargo-deny 0.20.2 的 `--offline` 是全局标志，必须放在子命令前面；照注释敲会被参数解析拒绝。`ci/run.sh:168` 用的是对的形态（`cargo deny --offline check`），所以门本身没坏，坏的是给人照抄的那行注释。

**证据**

```
$ cargo deny check --offline
error: unexpected argument '--offline' found
  tip: to pass '--offline' as a value, use '-- --offline'

$ cargo deny --offline check
advisories ok, bans ok, licenses ok, sources ok      # exit 0
```

**evidence**
- `deny.toml:6`、`ci/run.sh:168`
- 两条命令的实测输出

---

## 12. bans/advisories 的档位让门不会因重复版本或 yank 变红

**where** `deny.toml:4`、`deny.toml:29`

**问题** `multiple-versions = "warn"` 与 `yanked = "warn"` 意味着这两类问题永远不会让 `cargo deny check` 返回非零。F4 处置单说的「deny 四类全绿」在字面上成立，但门对重复版本与 yank 完全没有约束力——实测输出里确实有重复版本警告，门仍报 ok、exit 0。

**证据**

```
$ cargo deny --offline check
warning[duplicate]: found 2 duplicate entries for crate 'unicode-width'
    … 0.1.14（miette ← cedar-policy ← af-credential） / 0.2.2（pretty ← cedar-policy-formatter）
…
advisories ok, bans ok, licenses ok, sources ok
EXIT=0
```

**evidence**
- `deny.toml:4,29`
- `cargo deny --offline check` 实测输出（warning[duplicate] 若干条，末行四类 ok，EXIT=0）

---

## 13.（旁证，不在本面）`af_sampling_invariant_input` fuzz 目标没有 bin 声明，跑不了

**where** `fuzz/Cargo.toml:34-52`

**问题** `fuzz_targets/af_sampling_invariant_input.rs` 与对应 corpus 都在（提交 814bdb6），但 `fuzz/Cargo.toml` 的 `[[bin]]` 只声明了 3 个（workorder_order_parse / effect_gateway_idempotency / graph_schema_parse）。cargo 不从 `fuzz_targets/` 自动发现 bin，所以这个目标不是构建目标，`cargo fuzz run af_sampling_invariant_input` 无从谈起。

**证据**

```
$ ls fuzz/fuzz_targets/                       # 4 个文件
$ cargo metadata --manifest-path fuzz/Cargo.toml --no-deps | (取 targets 名)
['af_fuzz', 'af_effect_gateway_idempotency', 'af_graph_schema_parse', 'af_workorder_order_parse']
```

**evidence**
- `fuzz/Cargo.toml:34-52`（三个 `[[bin]]`）、`fuzz/fuzz_targets/af_sampling_invariant_input.rs`
- `cargo metadata --manifest-path fuzz/Cargo.toml --no-deps` 实测 targets 列表

---

## 声明与代码的差距（作者 RESULT 的说法 → 代码现状）

| 声明出处 | 说法 | 代码现状 |
|---|---|---|
| `crates/af-engine/RESULT-UPDATE.md:61` / `README.md:63,65` | 编排层先调 `approve_decision` 再调 `release` | 编排层从不调 `approve_decision`；审批门是裸 signal（`workflow.py:149-151`）；且该 op 与订单无绑定（§2） |
| `crates/af-engine/RESULT-UPDATE.md:3` + `README.md:3` | 订单快照「整单进、整单出」 | 未知字段被吞且不回吐（§3） |
| `deny.toml:5-6` | refresh 脚本同步到 cargo 默认库位，检查一律离线 | 同步段不可达，路径也与 cargo-deny 0.20 的哈希目录不符（§8） |
| `deny.toml:30` | `wildcards` 是为 path 依赖放行 | 实际放行范围含版本/git 通配（§10） |
| `orchestration/workflow.py:72` | `with_deadline` 用于 E3「新前置条件下谁会被拒」 | 引擎 release 门不读 SLA，该开关无判定效果（§5） |
| `crates/af-workorder/RESULT-UPDATE.md:11` 「continue_as_new 只替换 run_id」 | run 层随 CAN 变化 | 线协议无 run_id 入口，壳里恒为 ""（§6） |
| `8599948` 提交信息「deny 四类全绿」 | 供应链门有约束力 | 实测 ok 但 duplicate 警告不拦、yanked 不拦（§12） |

## 没查 / 没跑到的

- W3E 的 Temporal 集成复跑（`client.py start/approve/wait`，:7240 dev server）本轮未执行；§1 中 `data/engine_calls.log` 是作者那轮跑留下的本地未入库文件，我用它作为「作者自己的跑也如此」的佐证，不是本轮复现。
- `cargo deny` 的联网刷新路径本轮无网络，未跑。
- `fuzz` 工作区的实际编译与 `cargo fuzz run` 未跑（需要 nightly/libfuzzer），§13 只用 `cargo metadata` 证明了目标不在构建集合里。
- `orchestration/ledger.py:4` 注释「与形态B的 ledger.py 代码相同，仅数据文件不同」——`orchestration/` 下只有一份 `ledger.py`，形态B 在 `poc/poc-bridge/form_b/`，我没逐行比对，暂不列发现。
