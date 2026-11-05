# changelog-draft（拟由主代理统一追加到 `计划/变更日志.md`，本批不改原文件）

```markdown
2026-11-05 · POC5 静默子集密钥作废（feature/poc5-key-revocation）：公开仓曾暴露的 5 件 POC 私钥前向移出（git rm，历史不动，禁 rewrite/force push）——poc/poc-rekor/keys/log-ed25519-priv.pem、poc/poc-rekor/keys/cosign-gen/trace-signer.key、poc/poc-cas/cosign.key、poc/poc-cas/verify-tr/tr-key.key、poc/poc-cas-rust/casr-sign.key。配对公钥与 poc-rekor 私有信任根移入各 poc revoked/ 档案（不删，留作证物）。三 poc README 增「2026-11-05 作废登记：对应私钥曾于公开仓暴露，任何以此链验证的制品一律不可信」。消费面：ci/run.sh step6 改为 fail-closed（在役 tr-key.pub 缺席；放行面可读原因 key revoked；archived 链仅作验证器活性，篡改例仍必拒）；poc-rekor/verify/main.go 拒 revoked/ 路径公钥；纯文档（RESULT/REPORT/VERIFY-TRUSTED-ROOT/ci/REPORT/POC_SUMMARY/红队 F2 段）改写为作废声明，史实正文保留。五门：cargo test -j4 --workspace 1617/0/9；fmt --check --all exit 0；clippy -D warnings exit 0；clippy_gate PASS（0 条/0 签名，基线未动）；gate_inputs 14 件钉对账一致。判据正本/基线/白名单零改。例外件：无。遗留：git 历史与公开仓/镜像面仍含私钥材料，仓面止血由运营者另行处置；tools/mirror-denylist.paths 在 origin/product/main 不在场，本批未造该仓面件。
```
