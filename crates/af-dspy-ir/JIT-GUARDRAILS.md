# JIT 护栏

两条纪律挂在 `af-dspy-ir` 上，给 JIT 优化器在评测和修复之前调用。schema v1 的字段、DI 码和 DSPy 存档互转保持原语义。护栏不新写一套校验，也不按 `type_name` 复核 demo 值。

时刻来自 [`Clock`](src/clock.rs)。判定不读这个时刻。它只抄进账本，作为这一次判定的审计戳。测试注入 [`ManualClock`](src/clock.rs)，生产用 [`SystemClock`](src/clock.rs)。回拨、停住或向前拨这个钟，都不会把已经写下的拒绝改成放行。

## 挂接点

| 时机 | 函数 | 放行凭证 |
|---|---|---|
| best-of-N 评测之前 | `Guardrails::screen_before_best_of_n` | `AdmittedCandidate` |
| 修复动作之前 | `Guardrails::judge_repair` | `RepairPermit` |

两个凭证都没有公开构造。账本的追加也没有公开函数。`Ledger::from_json` 只恢复历史行，不把历史行换成凭证。

评测循环只消费 `ScreenBatch::admitted`。修复驱动只在 `RepairVerdict::permit` 有值时动手。

## 护栏一：静态 schema 预筛

优化器产出的候选先过 schema，再谈分数。不合格候选不进入 `admitted`，因此没有评测入口，也没有分数可以回灌。

一个候选是 `Candidate { id, ir }`。`ir` 两种形态：

- `CandidateIr::Program`：对值调用 `Program::validate`。检查版本、空标识、重复、签名与 demo 引用、字段名、demo 是否盖住字段表、`type_name` 非空、`config.n` 至少为 1、predictor 名不是 `metadata`。
- `CandidateIr::Json`：先走 `parse_program`。信封、未知键、JSON 类型和上面同一套语义校验都在这条路径上。版本不是整数 1 时，与解析器一样只报信封错误。

`validate` 一次收齐全部语义错误。预筛按这个顺序原样抄进原因，不截断，不改写成另一套码。

同一调用里的每一行共用当时的 `Clock::now_ms`。放行顺序等于输入里合格候选的相对顺序。

身份原因写在 schema 原因前面，顺序固定：

1. 批次 id 为空白（`JG1003`）。整批不放行。
2. 候选 id 为空白（`JG1001`）。空白 id 彼此不算重复。
3. 同一批里非空候选 id 再次出现（`JG1002`）。第一次出现仍按 schema 决定；后来的一律拒绝。
4. schema 失败时的 DI 码，顺序与 `validate` / `parse_program` 一致。

身份原因和 schema 原因可以同时存在。schema 已通过、身份失败时，账本只记身份原因，程序 id 仍写入。空批次不写账本。

拒绝行上的 `program_id`：值形态用程序里的 id（可以是空串）；JSON 形态在解析失败时，仅当根对象的 `program_id` 是字符串才记下，否则为 `null`。

放行行写下 `screen_admit`，并交出当时校验通过的程序副本。调用方事后修改自己手里的那份程序，不会改到这个副本。改过的副本是新候选，要再过一次预筛。

## 护栏二：repair_only_on_error

输入是一次运行结果 `RunResult`：运行 id、失败码分类、这次运行有没有留下评测分数。输出是允许修复或拒绝。账本不记录分数的数值。

`FailureClass::parse` 只认精确字符串：

| 文本 | 分类 |
|---|---|
| `E_INFRA` | 基础设施崩溃 |
| `E_SUBJECT` | 候选程序自身崩溃 |
| `E_SCORE`、`E_LOW_SCORE` | 低分 |
| 其他文本，含大小写不同、首尾空白、前后缀 | 其他失败码 |

调用方也可以直接构造枚举。`Other("E_INFRA")` 与 `Other("E_SCORE")` 在判定前按上表归一。低分枚举写进账本的码是 `E_SCORE`；`Other("E_LOW_SCORE")` 仍记下原文 `E_LOW_SCORE`。

判定顺序固定，先命中的为准：

1. 运行 id 为空白 → 拒绝，`JG2004`，消息「run_id 为空，拒绝修复」。低分或崩溃码都不能越过空 id。
2. 分类是低分 → 拒绝，`JG2001`，消息「低分类一律拒绝修复」。`scored` 是真是假都一样。
3. 分类是崩溃，且 `scored == true` → 拒绝，`JG2003`，消息「崩溃码 E_INFRA 附带了评测分数，拒绝修复」或把码换成 `E_SUBJECT`。有分数说明评测已经发生，这次结果不是纯崩溃。
4. 分类是崩溃，且 `scored == false` → 允许。凭证上的码是 `E_INFRA` 或 `E_SUBJECT`。账本行 `repair_allow.scored` 固定为 `false`。
5. 其余（没有失败码、其他失败码、空失败码）→ 拒绝，`JG2002`。有没有分数都走这一条。消息分别是「没有崩溃码，拒绝修复」「失败码 `<原文>` 不是崩溃类，拒绝修复」「失败码为空，不是崩溃类，拒绝修复」。

空失败码在账本里写成 `failure_code: null`，这样这份文档还能被读回。

## 账本

进程内追加。一行是一次预筛或一次修复判定。序号从 1 连续递增。调用方用 `Ledger::to_json` 保存，用 `Ledger::from_json` 再交给 `Guardrails::restore` 继续追加。序号接着长。

文档信封是 `ledger_version`，当前只接受整数 `1`。它不是 IR 的 `version`。缺字段、字符串、小数、0、2 都整份拒绝。

四种 `event.kind`：

| kind | 何时写入 | 读回时的结构约束 |
|---|---|---|
| `screen_admit` | 预筛放行 | `batch_id`、`candidate_id`、`program_id` 都非空白 |
| `screen_reject` | 预筛拒绝 | `reasons` 至少一条，每条 `code` 与 `message` 都非空白 |
| `repair_allow` | 修复放行 | `code` 只能是 `E_INFRA` 或 `E_SUBJECT`，`scored` 只能是 `false`，`run_id` 非空白 |
| `repair_deny` | 修复拒绝 | `reason_code` 只能是 `JG2001`–`JG2004`，`message` 非空白；`failure_code` 为 `null` 或非空白字符串 |

未知 kind、多余的键、断号、从 0 起号，读回失败。`at_ms` 允许为负，按注入值原样保存。

```json
{
  "ledger_version": 1,
  "entries": [
    {
      "seq": 1,
      "at_ms": 42,
      "event": {
        "kind": "screen_reject",
        "batch_id": "bo-1",
        "candidate_id": "c2",
        "program_id": "qa-cot",
        "reasons": [
          { "code": "DI1002", "message": "predictor `generate_answer` 引用了不存在的签名 `MissingSig`" }
        ]
      }
    },
    {
      "seq": 2,
      "at_ms": 42,
      "event": {
        "kind": "repair_deny",
        "run_id": "run-1",
        "failure_code": "E_SCORE",
        "scored": true,
        "reason_code": "JG2001",
        "message": "低分类一律拒绝修复"
      }
    }
  ]
}
```

## 绕过后果

调用方绕开挂接点、自己调用 `validate` 或 `parse_program` 然后直接评测时，坏 IR 会得到分数。分数回到优化器之后，优化器从指标里学习 schema 形状，评测反馈洞重新打开。预筛的作用就是让不合格候选在出分之前死掉。

评测使用调用方事后改过的程序、而不是 `AdmittedCandidate` 里的副本时，账本上的 `screen_admit` 对不上实际跑的字节。审计无法证明进了评测的就是过了 schema 的那一份。

低分触发修复、且不持有 `RepairPermit` 时，指标在驱动改写。账本上这次运行没有 `repair_allow`。把低分标成 `E_INFRA` 同时把 `scored` 设为 `false`，判定器看不见这个谎：失败码和 `scored` 必须来自运行时，不能来自优化器。`scored` 如实为 `true` 时，同一条崩溃码走 `JG2003`，仍然拒绝。

手改账本把拒绝涂成 `repair_allow` 时，码不是崩溃类或 `scored` 为 `true` 的文档读不回来。涂成一份结构合法的放行行，也发不出 `RepairPermit` 或 `AdmittedCandidate`。凭证只在当次挂接点调用里发出。把 `screen_reject` 的 `reasons` 清空，文档同样读不回来，拒绝原因不能被抹掉还继续当账本用。

不把 `to_json` 的文档留下，进程结束后代账本就没了，凭证上的序号失去对账对象。

用睡眠或墙钟去“等”一个放行不会生效。时刻不在判定式里。

## 审计点

- 一次 `screen_before_best_of_n` 的每个候选对应账本里连续的一行，这些行的 `at_ms` 相同，`batch_id` 相同。空批次没有新行。
- `AdmittedCandidate::ledger_seq` 指向一行 `screen_admit`，`candidate_id` 与凭证 id 一致，`program_id` 与副本的 `program_id` 一致。评测集合等于该批 `admitted`，不多不少。
- `RejectedCandidate::ledger_seq` 指向一行 `screen_reject`，`reasons` 与凭证上的原因一致。这批的 `admitted` 里没有这个 id。
- `RepairPermit::ledger_seq` 指向一行 `repair_allow`：`code` 是 `E_INFRA` 或 `E_SUBJECT`，`scored` 是 `false`，`run_id` 与凭证一致。修复动作拿不出这个凭证就不发生。
- `JG2001` 是被拒绝的低分修复请求。`JG2003` 是崩溃码上带了分数的请求。`JG2002` 是其余非崩溃。`JG2004` 是无法挂到一次运行上的请求。
- 读回后的序号仍从 1 连续。中间插入、删行、改放行码，都会在 `from_json` 失败，或者发不出新凭证。

## 本护栏不做什么

预筛和判定都不修改程序，不调用模型，不计算指标，不把 demo 值按 `type_name` 再判一遍。修复动作本身不在这个 crate 里。DI1001 到 DI1021 的含义与分配保持不变，本波次没有新的 DI 码。
