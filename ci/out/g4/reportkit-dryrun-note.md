# G4 Phase 2 干跑与 15 稿 digest 终态核对记录（2026-10-25，@5c887a1 注册面 63 条）

## paper_report 图干跑（判据 1）
```
$ af-compiler-plan --registry af/packages af/packages/reportkit/graphs/paper_report@1.0.afb
→ exit 0，stderr 空；双跑（in-test + CLI）逐字节一致
plan_id        = plan.paper_report@1.0+13a1799adc04
registry_digest= 63c3478e9321e1c58318620a592e9953ef34cee3a5d19ea4c3300a722fafe15b（63 条终态）
node_exec      = ingest / study / study.cite_check / study.reader / study.rubric_check / study.writer
reader artifact_contract = [notes_out, sources(envelope_of=source_snapshot)]（裁决5 记账面）
```

## 15 份对齐计划快照重跑 + digest 行更新（裁决8，G4 终态一次到位）
```
15/15 exit 0；更新前核对：每份与快照 diff 恰 registry_digest 一行
  d5f3bd3aa0f7ea65…d57977 → 63c3478e9321e1c5…afe15b（15 份全同），plan_id 逐字不变
快照文件（计划/研究组/对齐计划快照/*.plan.json，15 份）digest 行已更新
  （字节级单串替换；备份 /tmp/g4r-snapbak）
更新后复核：15/15 重编译产物与快照 cmp 逐字节全等
```
