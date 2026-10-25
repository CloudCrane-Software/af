# G4 Phase 1 干跑与 digest 联动核对记录（2026-10-25，@b77705f 注册面）

## E2E 图干跑（判据 0）
```
$ af-compiler-plan --registry af/packages af/packages/sweng/graphs/fix_swe_task@1.0.afb
→ exit 0，stderr 空；双跑 cmp 逐字节一致
plan_id        = plan.sweng_fix_swe@1.0+535e358b13cb
dsl_digest     = 535e358b13cbad592cb89799d0928ce4d8cfb239e20e6fc4d2a9e20d44bbbff3
registry_digest= d507bf61f6c34a0d90149ab4b99384b6db14bad1285d310e4ee23a4d8400456e
plan bytes     = 8748
```

## 15 稿对齐计划快照重跑（裁决8 逐位一致性）
```
15/15 exit 0；每份与 计划/研究组/对齐计划快照/<名>.plan.json diff 恰一行:
  compiled_from.registry_digest: d5f3bd3aa0f7ea65eb66f6534b22c255ea2f0860c1602e8adad290fd52d57977
                             → d507bf61f6c34a0d90149ab4b99384b6db14bad1285d310e4ee23a4d8400456e
plan_id 与其余字段逐位相同（digest 不入 plan_id 派生——G3 §① 先例复现）
快照文件物理更新与 N3 登记表联动：G4 终态（Phase 2 后）一次到位（RESULT §4/§7）
```
