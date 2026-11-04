# PoC-E 代码域适配器（lease→worktree→patch→门→gitea 合入→订单工件面）

既有件的接线，不是新机制面：

- **lease 面**：全部走 `poc/poc-supply/lease_store.py` 既有 CLI（`acquire`/`return`/
  `heartbeat`/`reap`/`show`，幂等、holder/TTL 守卫语义不改）；水位预检走
  `poc/poc-supply/disk_post_used_preflight.py`（exit 0=投放 / 2=拒单 / 1=输入错），
  观测采集走 `poc/poc-supply/collect_resource_observation.py`。lease DB 共用
  `af-state/resources/leases.db`（同一租约权威，Σactive 预留才进水位真值）。
- **门**：`gate_f1.sh` = af 仓 `ci/run.sh` 的**可跑子集**（step1 fmt / step2 clippy /
  step3 test，同款 step-计时-SUMMARY 语义、失败不中断跑完收全量诊断）。af 仓全
  管道的其余步绑定 af 资产与判据正本（cue/cosign/deliverables/gate-inputs 钉），
  在样例仓上不可跑——不伪造，如实登记。clippy 取 `-D warnings`（样例仓无基线
  文件，fail-closed；与 af 仓「基线门只拦新增警告」的语义差异如实登记）。
- **合入**：gitea `git push` 分支 → PR API 建单 → merge API 合入。合入前执行
  **integrator 四项绑定复核**（`four_binding_check`）：
  `base_sha`（分支父提交=记录值）/`source_sha`（远端分支头=记录值）/
  `tested_tree`（`source_sha^{tree}`=门跑过的树）/`suite_digest`（门脚本 sha256
  =记录值），外加 gate verdict 严格 == `pass`（af-workorder `gate_admit.rs`
  同词汇表，未知词一律非绿）。任一不符=拒绝合入。gitea Actions runner 不可用
  时以复核制替代，门不跳过。
- **订单账**：af-order CLI `submit`（stub 单元一次性驱动 Created→…→Closed），
  工件面挂 `state/orders/<order_id>/artifacts/`（artifacts.json + patch +
  gate_result + manifest.sha256）；`ledger.jsonl`/`mes-events.jsonl` 由 af-order
  驱动写，本适配器不手改账本。

## 生命周期绑定（lease ↔ worktree）

运行足迹固定 `<run-root>/<lease_id>/`（repo 克隆 + worktree +
`lease-binding.json`）。`release`（还）与 `reap`（过期回收）都移除整个足迹目录：
lease 过期后 worktree 不留存为「活」件。`reap` 先跑 `lease_store.py reap`
（active→expired 既有语义），再清掉全部非 active lease 的足迹；可独立执行、
可复跑，作「租约过期可回收」的独立验证。

## 用法

```bash
# 端到端一单（计划 JSON 字段见 codeadapter.py cmd_run / 30-工件 pocE e2e 实例）
python3 -I poc/poc-e-codeadapter/codeadapter.py run \
  --plan <plan.json> --run-root af-state/poc-e/runs --evidence-dir <evidence-dir>

# 还 lease（先移足迹再归还）
python3 -I poc/poc-e-codeadapter/codeadapter.py release \
  --run-dir <run-root>/<lease_id> --leases-db af-state/resources/leases.db --holder poc-e

# 过期回收（lease_store reap + 清非 active 足迹；可复跑）
python3 -I poc/poc-e-codeadapter/codeadapter.py reap \
  --run-root af-state/poc-e/runs --leases-db af-state/resources/leases.db --out <out.json>

# 门（af ci/run.sh 可跑子集，可独立复跑）
bash poc/poc-e-codeadapter/gate_f1.sh <worktree> <out-dir>   # 0=绿（SUMMARY 含 PIPELINE-GREEN）
```

## 测试

```bash
python3 -I poc/poc-e-codeadapter/tests/test_codeadapter.py
```

## 边界（如实）

- 门是可跑子集，不是 af 全管道（不可跑步见上，全量门只在 af 仓自身跑）。
- gitea Actions：样例仓放了 `.gitea/workflows/f1.yml`；runner 不可用时 PR 页
  面与 API 均无 run，走 integrator 四项绑定复核制合入（见 30-工件 pocE e2e）。
- 样例仓 `opchadmin/af-poc-e-sample` 专供 PoC-E 终验（靶子，非产品本体）；
  af 产品主线 product/main 不作合入靶。
- 样例仓门跑在 af 工作树目录下，rustup 会继承仓内 rust-toolchain.toml 的工具
  钉（1.99.1）——与 af 仓同工具链，属环境事实非新增依赖。
