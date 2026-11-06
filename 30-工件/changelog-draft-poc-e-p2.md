# changelog-draft（拟由主代理统一追加到 `计划/变更日志.md`，本批不改原文件）

```markdown
2026-11-05 · PoC-E 适配器 P2 两件（feature/poc-e-adapter-p2）：poc-e-fix 终审登记两件 P2 全修——① GiteaClient.api 增捕 urllib.error.URLError（含 reason）包装为 AdapterError PA11 族（codeadapter.py:390；HTTPError 路径不变）；连接失败仍 fail-closed（合入不落、订单不 Closed），仅错误分类学修正。夹具 127.0.0.1:1：修前裸 URLError(ConnectionRefusedError 111) 逃逸，修后 `PA11 gitea API GET /version -> URLError: [Errno 111] Connection refused`。② cmd_run finally 内 lease return 包 try/except（codeadapter.py:601-625）：归还失败时保留在飞异常（原 PA14 优先），归还失败仅 WARN 记 ledger.jsonl（kind=warn/level=WARN）与 e2e.json（lease_return_warn）；无在飞异常时归还失败仍抛。夹具「门红+归还失败」：修前红因被替换为 PA10 injected lease return failure，修后红因=PA14 且 WARN 落盘。夹具 pytest tests/test_codeadapter.py 14 passed（既有 12 例不删 + URLError→PA11 / 归还失败保 PA14 两例）。例外件：无。遗留：无（本批两件 P2 收口）。
```
