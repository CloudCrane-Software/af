# attempt1-quota429 —— EXP-1 repeat 轮试跑被 429 全量拒绝的实测证据

- 时刻：2026-11-06 00:52:53 起（anolis，时区 +0800）
- 执行面：本件 `exp1-repeats-submit.sh`（同款 harness：af-order + glm-cp + 冻结任务包）
- 结果：前 16 单全部失败，失败模式一致——端点 HTTP 429，provider 错误码 **1310**
  「您已达到每周/每月使用上限，您的限额将在 **2026-11-06 14:16:36** 重置」

## 件清单

| 文件 | 内容 |
|---|---|
| `submit.log` | 试跑提交脚本的完整 stdout（7 项预检全过 + 起批行 + 失败明细） |
| `E1R-*.result`（16 份） | 每单一行结果（`exit=1 status=submit_failed order_state=InProgress wall_s=…`） |
| `probe-ledger.jsonl` | **单单探针**（01:01:05）订单账本原文：429 / code 1310 / 重置时刻 14:16:36 逐字在案 |
| `probe-af-order.log` | 同探针的 af-order stderr 原文 |

## 读法与定性

1. 这**不是** harness 缺陷：`submit.log` 显示 7 项预检全过（profile 定义命中 / 密钥通道
   `KEY_OK len=49` 零明文 / 磁盘余量 8937MB / 144 个 plan.json 全部命中冻结版 sha256 /
   72 项包清单一一对应 / af-order 与 af-engine 在位）。
2. 这**不是**实验设计问题：预注册分配表 `round=="repeat"` 恰 72 行，臂集合 A0/A1/A2，
   任务包与主轮同款（plan.json 冻结版逐字节复用）。
3. 这是**通道配额窗**问题：provider 侧 coding plan 达每周/每月上限，限额 14:16:36 重置。
   处置见 `../../口径-EXP1-repeats-v1.md` §七 偏差①（保留证据、正式批换新状态根
   `/data/bench/exp1-state-r2r`、14:16 后自动起批）。
