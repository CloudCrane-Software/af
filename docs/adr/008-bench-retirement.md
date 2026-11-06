# ADR-008: 旧 bench 基础设施退役纪律（单一入口终态）

- 状态：已裁决（2026-10-24，编排者；依据用户裁决"最后的 bench 跑完后必须全部退役，功能完全并入 af 工厂底座，以后不存在两个入口"）

## 裁决

1. **终态**：唯一下单入口 = af 底座（af-order 门面 → af-engine 门控 → af-execline 执行 → af-mes 记账）。bench 与业务任务同通道。
2. **退役对象**：tools/factory-sched.py、tools/mcode-dispatch.py、tools/bench-mes.py、af-bench 独立 CLI 直跑形态、MES Python 旁路直 INSERT；factory-keepalive 看护清单同步收敛（改看护 af 执行面）；factory-watch 判据随切流更新。
3. **退役前置条件**（引用 计划/研究组/在飞bench盘点-20261024.md §3，逐条可检验）：
   - 三测试扩量批收官（results.jsonl 6h 无新行 + 无在跑进程 + anolis 0 容器三重确认）；
   - 新执行面影子对拍一致（同题同判）；
   - main-sched 12 个永久阻塞臂（挂在 6 个终态 failed relay 上）由运营者裁决补跑或放弃——**属运营者待裁决项，不阻塞退役准备**。
4. **流程**：影子对拍 → 切流（旧脚本停用进观察期，保留一行回滚命令）→ 观察期（跨天稳定）→ 归档（tools 脚本移 参考/legacy/bench-infra-retired-2026MMDD/，附退役报告）。
5. 切流窗口避开每小时 :17 的 cron 巡检；停机顺序按盘点文档 §4（先 sched 后 keepalive——keepalive 必须晚于其看护对象停，否则 30s 内复活一切）。
6. 全程登记：计划/30-遗产处置.md 删除记录表 + 变更日志 + 技术债 D-2/D-3/D-39 销项。

## 不退役（保留资产）

factory-watch.py（观测面，判据更新后转看护新执行面）、出题三件套管线（冷/合成/改造，产线资产）、swev-judge 判分核（字节冻结，被新执行面以同契约调用）、/tmp 工厂运行时件直至观察期结束。
