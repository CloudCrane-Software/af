# poc-rekor revoked 档案（2026-11-05）

配对公钥与私有信任根留作证物。对应私钥已 `git rm`（只存在于 git 历史）。任何以此链验证的制品一律不可信。

| 档案 | 原路径 | 配对私钥（已 git rm） |
|---|---|---|
| `log-ed25519-pub.pem` | `keys/log-ed25519-pub.pem` | `keys/log-ed25519-priv.pem` |
| `trace-signer.pub` | `keys/cosign-gen/trace-signer.pub` | `keys/cosign-gen/trace-signer.key` |
| `trusted-root-private.json` | `keys/trusted-root-private.json` | 内嵌 log 公钥，由 `log-ed25519-priv.pem` 派生 |
