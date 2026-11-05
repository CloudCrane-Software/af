# 差距 / 未做（风险#3 收口后仍登记的边界）

本批已按缺陷单第一方案把数量路径 fail-closed。下列是有意未做，不是漏修。

1. **未打通 deduct 与 acquire/release**。Concurrency 占用仍只走 `acquire`/`release`；
   `try_consume`/`route` 碰到 Concurrency（含 Min 嵌套、Max 选中）返回
   `MeterError::Unsupported`。缺陷单给了两条路，本批走「拒绝静默成功」。
2. **未抬 PoC-G 配方限制**。`poc/poc-supply/validate_machines.py:193` 仍要求
   并发上限 `kind: Stock`。缺陷单写明「测试齐后再抬」——测试已齐，抬配方是下一跳。
3. **`deduct` match 里 Concurrency 空臂仍在**。`quantity_consume_unsupported`
   在 match 前已拒绝，该臂到不了；留下是为了 enum 穷尽，不是数量路径入口。
4. **单消费者 / 无持久化** 沿用 crate 既有边界，本批未扩。
