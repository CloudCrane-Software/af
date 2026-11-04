# 观察项：R3 无库总量下限（非 fail-open 洞，审计面事项）

`ci/strategies_check.py:351-368`（`cmp_counts`）只对账「登记值 = 机算值」，
不锁库总量下限。空库被 `main()` 的「无条目 YAML」硬红拦住（约 L440），
但 **1 条且 counts 同步** 即可全绿（schema/index 面）。

本夹具把门脚本拷到自包含树（`Path(__file__).parent.parent` 落到本目录），
只留 1 条 `not_applicable` 条目 + 与之匹配的 `index.yaml` counts，
`--skip-compile` 跑 schema/index 面。预期：exit 0。

这不是「门被绕过」——R0 仍要求被测对象入库，缩库必须经 git 历史。
登记为审计面事项，本批不改门（禁放宽；收紧下限属跨批产品决策）。

复现：

```
python3 -B poc/poc-p3-a1-strategy-dsl/raw/fail-open-r3-nofloor/ci/strategies_check.py --skip-compile
```
