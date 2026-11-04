# 指纹（test/risk5-fault-injection，外派验收面）

- 基线：`origin/product/main` = `56ff52a57c022983d74d994fd4e1c63e98f45235`
  （fix(ci): 门输入钉再重钉（sat-cast 销项改了另两件判据正本））
- 工作树：`~/af-dev/af/wt/risk5-faultinject`（`git worktree add --detach` 自建，
  分支 `test/risk5-fault-injection` 指向基线；**未 commit / 未 push**，
  共享树分支指针未动）
- 改动面（untracked，全部新增、零修改既有件）：
  - `crates/af-order/tests/adjudicate_fault_injection_acceptance.rs`（新增测试，A1–A8）
  - `poc/poc-risk5-fault-injection/`（本目录：README / GAPS / FINGERPRINT / raw）
  - `.offload/` 为环境自带（会话开始前已存在，非本批产物）

## 文件 sha256

```
f05ad34373c92182b2a3497989bd2e46ca04c4587677cf41cbd652074a8f7477  crates/af-order/tests/adjudicate_fault_injection_acceptance.rs
b163bae6a7a6cdd505f1ea007e6dc973e697149ebe9a68bc85e50415b027ed8c  poc/poc-risk5-fault-injection/README.md
3603a31d792c1159849341627cb3137d2e3600d4cc7cb74a9372d6ba92c67d4d  poc/poc-risk5-fault-injection/GAPS.md
```

（raw/ 下日志哈希以其文件为准；gate2-final-test.log 为终跑版。）

## 门读数汇总

| 门 | 结果 |
|---|---|
| 1 fmt | exit 0 |
| 2 test | 基线 1578/0/9 → 终跑 1586/0/9（+8，只增不减），exit 0 |
| 3 clippy | error 0 / warning 0，`clippy_gate.py` PASS（基线 0 签名零动），exit 0 |
| 4 零弱化 | ci/ 0 diff；clippy-baseline sha256 与钉一致（a48639e7…）；Cargo.toml/lock 0 diff；crates/ 0 diff（仅新增测试文件） |

## 验收结论

主干通过（W0–W3 + W1.5 部分结算 + admit 拒绝形态收敛/幂等）；差距 5 条
（G-1 Closed+Reject 不可收敛悬空 / G-2 毒账阻断 reconcile / G-3 补偿调用
副作用二次受理 / G-4 lease 库漂移留检 / G-5 受理账漂移双判读不对称），
详见 GAPS.md。
