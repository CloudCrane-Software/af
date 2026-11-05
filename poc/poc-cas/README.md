# poc-cas

历史实测报告见 `RESULT.md`、`VERIFY-TRUSTED-ROOT.md`、`VERIFY.md`。

## 2026-11-05 作废登记

对应私钥曾于公开仓暴露，任何以此链验证的制品一律不可信。

| 私钥（`git rm`，历史不动） | 配对公钥（移入 `revoked/` 作证物） |
|---|---|
| `cosign.key` | `revoked/cosign.pub` |
| `verify-tr/tr-key.key` | `revoked/tr-key.pub` |

在役路径不得再出现上述私钥或配对公钥。`ci/run.sh` step6 已改为 fail-closed：验签放行面必失败，可读原因为 `key revoked`。`verify-tr/trusted-root.json` 是 sigstore 公共实例 TUF 根（`cosign initialize` 产物），不是本 poc 密钥对，仍留在 `verify-tr/` 供验证器活性检查。
