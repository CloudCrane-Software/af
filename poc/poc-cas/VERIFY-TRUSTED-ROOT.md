# VERIFY-TRUSTED-ROOT.md — cosign 离线验签第二条路（--trusted-root）补测

对应 `poc-cas/RESULT.md` ④-5/⑤："纯离线验证必须 --insecure-ignore-tlog 或预置 --trusted-root（未测）"。本次全部真实执行，2026-10-19，cosign v3.1.3（`af/bin/cosign`）。工作目录 `poc/poc-cas/verify-tr/`（未动上级已有密钥对 cosign.key/cosign.pub）。

## ① 结论

**该路线成立**。`cosign verify-blob --key <pub> --bundle <bundle> --trusted-root <trusted_root.json>` 在 `sudo unshare -n`（无网络命名空间）下 **Verified OK、exit 0**，不需要 `--insecure-ignore-tlog`，且**不依赖 `~/.sigstore` TUF 缓存**（HOME 指向无缓存的 /root 同样通过）。关键是它没有丢透明账本审计性：篡改 bundle 里的 tlog 条目后，离线验证照样以 `failed to verify log inclusion` 拒绝——rekor inclusion proof 是拿 trusted_root.json 里预置的 Rekor 公钥离线校验的。

同时**复现并确认了路 1 的死路**：即使 `cosign initialize` 刚刷新出完整新鲜的 TUF 缓存，断网 verify-blob 仍然强制在线刷新（`tuf refresh failed`，exit 1）——"只靠缓存"在 v3.1.3 行不通，预置 `--trusted-root` 文件是唯一的纯离线带 tlog 路径。

CI 可用的完整操作序列见②末尾。

## ② 步骤（复现序列）

**形状确认**（以 `--help` 实测为准）：
- `cosign initialize`：拉默认镜像 `tuf-repo-cdn.sigstore.dev`（RESULT.md 里写的 "tuf-rekor-cdn" 有误），写入 `$HOME/.sigstore/root/`；
- `verify-blob --trusted-root=''`：`Path to a Sigstore TrustedRoot JSON file`——就是一个文件参数；
- 该文件获取方式：`cosign initialize` 的缓存产物 `~/.sigstore/root/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json`（6787 字节，mediaType `application/vnd.dev.sigstore.trustedroot+json;version=0.1`，含 tlogs/certificateAuthorities/ctlogs/timestampAuthorities 四节）。

```bash
# 0. 在线准备（一次性，或定期重刷）
cosign initialize                                        # 2.6s, exit 0，刷 TUF
cp ~/.sigstore/root/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json ./trusted-root.json

# 1. 签名（在线，自动上传 rekor tlog）
COSIGN_PASSWORD=... cosign generate-key-pair --output-key-prefix tr-key
COSIGN_PASSWORD=... cosign sign-blob --key tr-key.key --bundle tr-bundle.json artifact-tr.txt
# → bundle mediaType: application/vnd.dev.sigstore.bundle.v0.3+json, 含 tlogEntries

# 2. 验证（可完全离线；trusted-root.json 与 cosign.pub 随验证方分发）
cosign verify-blob --key tr-key.pub --bundle tr-bundle.json \
    --trusted-root trusted-root.json artifact-tr.txt     # → Verified OK, exit 0
# 负例：篡改工件任意 1 字节 → exit 1
```

CI 要点：**第 0 步产物 trusted-root.json 是构建期工件**（建议随底座一起签名分发并定期重刷——sigstore 换 Rekor key 时它要跟着更新）；验证机不需要网络、不需要 ~/.sigstore。

## ③ 证据（命令原文与输出摘录）

**路 1（只靠 TUF 缓存）——死路，复现 RESULT.md 的坑**（缓存刚由 initialize 刷新成功）：
```
$ cosign initialize                       # exit 0, 2.6s；缓存含 targets/trusted_root.json 等 7 文件
$ sudo HOME=/home/pan-ding unshare -n cosign verify-blob --key tr-key.pub --bundle tr-bundle.json artifact-tr.txt
Error: setting trusted material: getting trusted root from TUF for new bundle verification:
  error getting live trusted root: failed to create TUF client failed to load metadata:
  tuf refresh failed: Get "https://tuf-repo-cdn.sigstore.dev/15.root.json":
  dial udp 192.168.1.1:53: connect: network is unreachable
exit 1
```
即：有新鲜缓存也不读，强制 "live trusted root" 在线刷新。

**路 2（--trusted-root）——断网通过**：
```
$ sudo HOME=/home/pan-ding unshare -n cosign verify-blob --key tr-key.pub --bundle tr-bundle.json --trusted-root trusted-root.json artifact-tr.txt
Verified OK
exit 0                                    # 无 --insecure-ignore-tlog

$ sudo unshare -n cosign verify-blob ...  # 对照：HOME=/root（无任何 TUF 缓存）
Verified OK / exit 0                      # 完全自包含，不依赖缓存

$ cosign verify-blob ... --trusted-root trusted-root.json ...   # 在线对照
Verified OK / exit 0
```

**负例**：
```
# 负例1：工件篡改 1 字节（cmp -l: "8 162 130"，h→X）
$ unshare -n ... verify-blob ... artifact-tampered.txt
exit 1（无 Verified 输出）

# 负例2：bundle 内 tlog 条目被换（logIndex → 999999999），证明离线仍在做 tlog 审计
$ unshare -n ... verify-blob --bundle tr-bundle-badtlog.json ...
Error: failed to verify log inclusion: not enough verified log entries from transparency log: 0 < 1
exit 1

# 负例3：trusted-root 文件损坏（内容 "garbage"）
Error: setting trusted material: loading trusted root: failed to proto-json unmarshal trusted root:
  proto: syntax error (line 1:1): invalid value garbage
exit 1
```

**网络前提实测**：`tuf-repo-cdn.sigstore.dev` 本次可达（TLS/HTTP 通，2.6s 完成整棵 TUF 刷新），签名时 rekor tlog 上传成功（bundle 含 tlogEntries）。上次 PoC 的"死代理"环境问题本次未复现。

## ④ 对最终方案的反馈

1. **两条离线路线的取舍定了**：`--insecure-ignore-tlog`（上次已测，纯本地密码学验证，丢审计）vs `--trusted-root`（本次实测通过，**保住 tlog 审计且零网络**）。要求"验证零网络"的环节应统一用 `--trusted-root`；它的运维代价是要分发并定期刷新一个 6.6KB 的 JSON。
2. **v3.1.3 的行为语义**：verify-blob 遇 bundle 时默认路径是"get live trusted root"（强制在线 TUF 刷新），本地缓存只用作刷新的信任锚，不直接用作验证材料；`--trusted-root` 一旦给出就完全短路 TUF 客户端。因此**缓存无法替代 trusted-root 文件**，CI 里不要指望预热缓存。
3. **离线 tlog 审计是真的**：inclusion proof 校验用的是 trusted-root 里预置的 Rekor 公钥 + bundle 内 checkpoint，不需要问网络（负例 2 实证）。也就是说该路线保住了"任何持 Rekor 公证人角色者可事后审计"的性质——这正是 `--insecure-ignore-tlog` 丢掉的东西。
4. **风险与维护**：trusted-root.json 里固化了 sigstore 公共实例的当前密钥集（Fulcio/Rekor/CT）；sigstore 轮换密钥后旧文件里的公钥仍在验证侧生效，需建立"定期 `cosign initialize` + 比对 + 重分发"的流程，并考虑给该文件本身做完整性保护（它若被替换成攻击者自己的根，整个审计失效）。
5. **修正一处旧记录**：默认 TUF 镜像是 `tuf-repo-cdn.sigstore.dev`（不是 tuf-rekor-cdn）。

## ⑤ 遗留问题

- trusted-root 的**刷新周期与轮换监测**未设计（sigstore 上游密钥轮换事件如何感知/告警，属流程问题非 CLI 问题）。
- 未测 keyless（Fulcio 证书链）场景下的 --trusted-root 离线验证——本次是 key+bundle 场景；keyless 时 certificateAuthorities 一节才被用到，预计同样离线可行，未实证。
- 未测 `cosign verify`（registry 内镜像签名，非 blob）的同样离线化；若 zot 场景要离线验镜像签名需另行补测。
- `--use-signed-timestamps`（RFC3161 timestamp authority，trusted-root 的 timestampAuthorities 一节）未测。
- 上次遗留的"`cosign sign`（镜像签名）+ zot referrer API"仍待补（与本次无关）。

## 附：本次产物（verify-tr/ 内保留）

- `trusted-root.json`（6787B，initialize 产物，--trusted-root 实测输入）
- `tr-key.pub` / `tr-key.key`（本次新生成密钥对，未动上级已有密钥对）
- `artifact-tr.txt` / `tr-bundle.json`（正例工件与 v0.3 bundle）
- `artifact-tampered.txt` / `tr-bundle-badtlog.json` / `bad-root.json`（三个负例输入，均为小文件）
