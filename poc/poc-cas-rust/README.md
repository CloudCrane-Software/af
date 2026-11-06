# poc-cas-rust

历史实测报告见 `REPORT.md`。

## 2026-11-05 作废登记

对应私钥曾于公开仓暴露，任何以此链验证的制品一律不可信。

| 私钥（`git rm`，历史不动） | 配对公钥（移入 `revoked/` 作证物） |
|---|---|
| `casr-sign.key` | `revoked/casr-sign.pub` |

在役路径不得再出现上述私钥或配对公钥。本 poc 无 CI 验签步消费该键；文档中的 `cosign sign --key casr-sign.key` / `cosign verify --key casr-sign.pub` 复现命令已作废，不得再当放行锚。
