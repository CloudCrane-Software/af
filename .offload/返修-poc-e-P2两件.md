# 外派任务书 · 施工面【返修·PoC-E 适配器 P2 两件】（grok 通道）

> 你是**施工员**（grok 通道）。poc-e-fix 批（feature/poc-e-adapter-hardening 已合主线）终审登记两件 P2 下轮必修，本批修掉。基线 = origin/product/main 最新（先 fetch）；新分支 **feature/poc-e-adapter-p2**。
> 出口已带代理；构建盘 CARGO_TARGET_DIR=/data/bench/targets/poc-e-p2。

## P2-1 · GiteaClient.api 只捕 HTTPError（codeadapter.py ~:388）
连接拒绝/URLError 等裸抛不包 PA11。修：捕获 urllib.error.URLError（含 reason）包装为 AdapterError（PA11 族），连接失败=fail-closed 方向不变（合入不落、订单不 Closed），仅错误分类学修正。夹具：造连接拒绝场景（如 127.0.0.1:1），修前裸 URLError 逃逸（红）/修后 PA11（绿）。

## P2-2 · finally 里 lease return 失败盖原红因（cmd_run ~:598）
`finally` 中 `lease(... return ...)` 无 try/except，归还失败会替换在飞的 PA14。修：finally 内 lease return 包 try/except——归还失败时**保留原异常**（原 PA14 优先），归还失败仅记 WARN 进 ledger/e2e.json（证据不丢）。夹具：造「门红+归还失败」双故障，修前红因被替换（红）/修后红因=PA14 且 WARN 落盘（绿）。

## 门
`cd poc/poc-e-codeadapter && python3 -m pytest tests/test_codeadapter.py -q` 全绿（既有 12 例不删+新增两例）；本仓 fmt（无 Rust 改动则免）；禁 commit product/main（push 新分支）；changelog-draft 落盘 30-工件/changelog-draft-poc-e-p2.md（仓内+工厂各一份）。

## 返回
两件修法（file:line）+先红后绿读数｜pytest 全量｜分支+HEAD｜边界。
