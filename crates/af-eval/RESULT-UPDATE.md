# RESULT-UPDATE.md — W5B af-eval 评测产线骨架

日期：2026-10-19。分支 `task-W5B`。实现提交 `c3133f4`。

## 结论

完成。新建 `crates/af-eval`，评测产线骨架按五条纪律落地，`cargo test -p af-eval` 19 个测试全绿（单元 12、集成 7、文档测试 0），clippy `-D warnings` 无输出。没有新的第三方 crate。

## 步骤

1. 纯规则：SHA-256（本 crate 实现，NIST 向量）、规范快照字节、payload 精确交集、四失败码路由、可注入时钟。
2. 账本：`Catalog` 把基准事件、配方事件、运行绑定和运行结果落到一个 SQLite 文件。阶段由事件重放得到，不另写一套状态机。
3. 三态：`create` 得到 draft v1；`freeze` 封存快照哈希；`retire` 只从 frozen 进入。draft 不能直接退役，也不能开跑。
4. 冻结后 `put_case` / `remove_case` 返回 `Sealed`，内容与哈希不变。`fork` 只接受已封存的谱系头，复制用例成下一版 draft。
5. 盲评：`RunView` 只有冻结快照和被评对象版本。`recipe_during_run` 在运行存在时一律 `InformationBarrier`。账本外的 `current_recipe` 仍可读最新配方。
6. 污染：开跑前做交集。命中则同一事务写入 `run_open` 与 `outcome=halted` 的 `run_close`，提交后返回 `Contaminated`，没有失败码，也不能再 `finish_run`。未命中才留下进行中的运行。
7. 失败码：`elapsed >= budget` 为 `E_TIMEOUT`，否则 `E_INFRA`，否则 `E_BENCH`，否则 `E_SUBJECT`，否则通过。时钟回退不写结果。
8. 只追加：五张表都有 UPDATE/DELETE 触发器，命中 `RAISE(ABORT, 'append-only')`。公开方法 `update_run_subject_version`、`update_run_outcome`、`delete_run` 把这个错误收成 `AppendOnly`，事务回滚。
9. 快照纪律：`run_open` 写入冻结快照哈希和 `subject_version`。重放时哈希必须与基准重算值一致，否则拒绝打开。
10. 开库：文件库 `journal_mode=wal`；内存库只能是 `memory`（与 af-sampling 相同）。两者都核对 `synchronous=NORMAL(1)`、`foreign_keys=1`、`busy_timeout=5000`。`user_version` 不是 1、声明 `ledger_id` 不同、半截库、表或触发器集合漂移都拒绝，不补表。

`Cargo.lock` 不在 `crates/af-eval/**` 里。不改它的话，`--locked` 锁不住新成员。diff 只有新增的 `af-eval` 包，依赖是锁里已有的 `rusqlite` 和 `thiserror 2.0.21`。根 `Cargo.toml` 未改（`members = ["crates/*"]`）。其他 crate 未改。

## 证据

```
$ cargo check -p af-eval --offline -j 4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.36s

$ cargo fmt -p af-eval -- --check
（无输出，退出码 0）

$ cargo clippy -p af-eval --all-targets --offline -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s

$ cargo test -p af-eval --offline -j 4
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored

$ git show --stat --oneline -1
c3133f4 实现 af-eval 评测产线骨架
 Cargo.lock                         |    8 +
 crates/af-eval/**                  | 3397 +
 12 files changed, 3405 insertions(+)
```

集成测试对应验收项：`draft_freezes_then_retires`、`frozen_content_rejects_edits_and_forks_a_new_version`、`contamination_hit_halts_and_miss_can_finish`、`failure_codes_stay_distinct_and_persist`、`append_only_ledger_rejects_update_and_delete`。另有盲评、重入回放（停机行与通过行的哈希和对象版本都还在）。原先没有 `af-eval`，这 19 个都是新测试。

## 反馈

1. 污染成员是用例 payload 的精确字符串，不是子串，也不做大小写或空白归一。最终方案若要防“改两个字符就绕过”，应换比较函数，不要放宽成默认子串，否则短 payload 会误伤。
2. 四个码互斥。超时压过装置故障：预算到点即 `E_TIMEOUT`，哪怕同时报了 infra。若产品要把“装置已明确报错”算成 `E_INFRA`，需要改 `route_failure` 的优先级并改测试，不能两套码同时落在一行。
3. 当前配方是只追加历史，最新一行才是当前。屏障在运行入口，不在配方表上。操作员接口和评测接口必须继续分成两个方法，合成一个带 `blind: bool` 的读取会变成旁路。
4. 绑定行（`run_open`）和结果行（`run_close`）分开，是为了开跑时就把哈希和对象版本钉住，停机和通过走同一张绑定。读成绩用 `run_record`，不要只扫 `run_close`。
5. 快照摘要是小写十六进制 SHA-256，覆盖谱系、版本和按 `case_id` 排序的用例。以后若改用工作区已有的 `sha2` 0.10，摘要字符串不能变。

## 遗留

1. 没有执行器。`ExecSignals` 由调用方交来，本 crate 只路由和落盘。
2. schema 只有版本 1。对不上就拒绝，没有迁移。
3. 交集不做 Unicode 归一。空输入视为未污染。
4. 公开 API 是单连接。`busy_timeout` 已按内核设成 5000ms，多进程并发写没有在本任务里压测。
5. fork 只在同一谱系上从最新封存版加一。不能从中间版本或另一条谱系分叉。
6. 未跑工作区全量测试。本任务只要求 `cargo test -p af-eval`。
