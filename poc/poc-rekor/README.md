# poc-rekor

历史实测报告见 `REPORT.md`。离线验证器源码在 `verify/`。

## 2026-11-05 作废登记

对应私钥曾于公开仓暴露，任何以此链验证的制品一律不可信。

| 私钥（`git rm`，历史不动） | 配对公钥 / 信任根（移入 `revoked/` 作证物） |
|---|---|
| `keys/log-ed25519-priv.pem` | `revoked/log-ed25519-pub.pem`、`revoked/trusted-root-private.json` |
| `keys/cosign-gen/trace-signer.key` | `revoked/trace-signer.pub` |

在役路径不得再出现上述私钥或配对公钥。`verify/main.go` 对 `revoked/` 下的公钥 fail-closed：可读原因为 `key revoked`，不得验签放行。文档中的 `rekor-server-posix --signer-filepath keys/log-ed25519-priv.pem` 与 `cosign verify-blob --key keys/cosign-gen/trace-signer.pub` 复现命令已作废。
