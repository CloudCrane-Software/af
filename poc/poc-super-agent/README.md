# poc-super-agent — Router 最小皮 + Trigger 事件层（PoC-D）

归属：v0.3 §二 阶段1 PoC D「事件层分工」+ T6 终裁落地。
验收口径：Trigger 收件箱 → Fast Path 投递 → Router(Slow Path) 产 DecisionArtifact
经 admit 生效，链路可跑（自检 36/36：原批 29 + 修复批 7 新检，tests/selfcheck.py；
原批两项高危〔收件端 secret 缺失空密钥降级 fail-open / 消费腿吃冗余 parsed body
可篡改派单〕已于 2026-11-03 修复批修死，见 30-工件/janitor-20261103/pocd-fix-*.md）。

```
webhook(HMAC) ──► tools/trigger-inbox.py（外层工作区，只读注册表+写 inbox）
                     │ af-state/triggers/inbox/<source>/<delivery-id>.json（原子）
                     ▼
        tools/event-router.py poll_triggers 消费腿（60s 主循环内，.router-STOP 同熔断）
                     │ 注册表映射 push→F1 / pull_request opened|synchronize→F2
                     │ order_id=hash(repo+suite+subject+recipe_digest) 确定性幂等
                     ▼ af-state/triggers/dispatch/<order_id>.json（派单记录，不执行门套件）
                     ▼
        cli/cli.py decide --dispatch …（无订单对话必拒 T6.1）
                     │ router/slow_path.py 六步 ── admit/guard.py 三禁最小集
                     │        ②③④⑤ 全调 bridge/（af-workorder/af-release-gate 现成 API）
                     ▼ af-state/triggers/slowpath/<order_id>/{decision-artifact,permit,trail}.json
        DecisionArtifact(Reviser/Revise) ─四层校验→ ReleaseGate::admit（release 唯一写者）
```

- 契约：`contracts/af.trigger.v1.md`（七字段+存储+HTTP 五态口径）
- 注册表：`registry/trigger-registry.v1.json`（正本；运行位=af-state/triggers/registry.json，
  由 tests/install_registry.py 安装并算定 recipe_digest；映射/阈值是数据不是代码）
- 桥：`bridge/`（独立 cargo 工程；策略不进桥，桥留显式 --role/--decision 仅供负例探针）
- 皮：`router/slow_path.py`（六步编排）、`admit/guard.py`（三禁最小集+Reviser 钳制）、
  `cli/cli.py`（decide/probe/status/chat——chat 必拒）
- **三禁辖界（勿扩大读）**：不持 shell/root、release 唯一写者、凭证只收紧——三条
  **只对本目录 poc/poc-super-agent/ 最小皮成立**（guard.py CAPABILITY_DENYLIST +
  selfcheck S4 AST 级断言）。在役 event-router 进程既有 root SSH+shell=True 能力面是
  **既有事实**，属遗留风险另档登记（30-工件/janitor-20261103/pocd-router-capability-debt.md），
  不得以最小皮 denylist 冒充整个进程无此能力。
- 自检：`tests/selfcheck.py`（正例链路+负例必红+安灯两态+修复批 7 新检，沙箱
  AF_TRIGGER_HOME 隔离，CHANGELOG monkeypatch 不写生产+Z1 全程 sha 前后一致断言）；
  `tests/event-router-poCD.diff`=在役路由器改动全量 diff（相对 `tools/event-router.py.20261103-pre-POCD.bak`：
  原批 322 insertions/1 deletion，修复批后全量 406/1；可复跑
  `git diff --no-index --numstat tools/event-router.py.20261103-pre-POCD.bak tools/event-router.py`）；
  `tools-mirror/trigger-inbox.py`=外层工具镜像（正本=工作区 tools/trigger-inbox.py）

MES complete→裁决单消费腿=PoC-C 职责，本目录不含（两批不共改一函数）。
ed25519 仍是 placeholder（开放风险#1，BOOT-1 换真，接口不变）。
