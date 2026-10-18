# RESULT.md — poc-cas：内容寻址存储落地实测（zot + crane + cosign，纯二进制无 Docker）

## ① 结论

**通过**（核心验收全部真实执行通过；有两处环境/版本层面的路径修正，均已如实记录）。

| ask 验收点 | 结果 |
|---|---|
| zot 起服（HTTP 5050、storage ./data、默认 GC） | ✅ `GET /v2/ → 200` |
| crane push v1 + v2（改 layer） | ✅ v1/v2 均推上（真实 alpine amd64 layer；因 docker.io 不可达改从 public.ecr.aws 拉取，manifest 用 OCI media type，见④-1/2） |
| 内容寻址：v1/v2 digest 不同 + manifest 可拉 | ✅ `d978230b…` ≠ `239511b5…`；两 manifest 均拉回成功 |
| cosign 离线签名/验签 + 篡改 1 字节失败 | ✅ 正向 `Verified OK`（封网亦可）；篡改后 exit 1 |
| GC 可控触发方式 | ✅ 找到并实测通过：`zot verify-feature retention`（不是留 M1，已闭环） |
| 结束杀 zot、保留 data | ✅ 端口 5050 已释放；`data/` 3.8M 保留 |

## ② 做了什么

1. **zot 起服**：`config.json`（distSpecVersion 1.1.1 / http 127.0.0.1:5050 / storage.rootDirectory=./data，GC 配置缺省不动）→ `zot verify` 合法 → `zot serve` 后台起，日志 zot.log（debug 级）。
2. **镜像 push**：`crane pull alpine:latest` 走 index.docker.io 连续 6 次被 connection reset（IPv6 链路；IPv4 也 000）。改用 `crane pull public.ecr.aws/docker/library/alpine:latest` 成功（3.85MB）。因 zot 拒收 docker manifest（见④-1）、crane push 只认 docker-save tarball（见④-2），最终用 ask 允许的备选路径：`crane append -f <真实 alpine amd64 layer> --oci-empty-base -t …:v1 --insecure` 推 v1；v2 = `crane append -f <标记 layer(line/REVISION)> -b …:v1 -t …:v2 --insecure`（多一层 → 内容变化）。
3. **内容寻址验证**：`crane digest` v1/v2 对比；`crane manifest` v1/v2 拉取；磁盘自检（blob 文件名==sha256(内容)）push 后 8/8、GC 后 4/4；两 tag 共享同一 layer blob（`e2de9651…`），crane 二次 push 时报 `existing blob`（跨 push 去重生效）。
4. **cosign 离线签名**：`COSIGN_PASSWORD=… cosign generate-key-pair` → `sign-blob --key cosign.key --bundle out.bundle artifact.txt`（bundle v0.3，自动带上 rekor tlog 条目）→ `verify-blob --bundle --key` → `Verified OK`；篡改第 26 字节（o→O）→ `invalid signature`，exit 1。封网（HTTPS_PROXY=死代理）复验：tlog 开着会去刷 TUF 根元数据而失败；加 `--insecure-ignore-tlog` 纯本地密码学验证通过。
5. **GC**：`zot --help` 确认无 `gc` 子命令；zot 官方文档（zotregistry.dev/v2.1.22/articles/retention/）给出可控触发：`zot verify-feature retention <config>`，"runs garbage collection and retention tasks"（GC 真执行，仅 retention 策略部分强制 dry-run；本地存储要求先停服避免 db 锁）。实测：`crane delete …:v1` 制造孤儿 → 停服 → `zot verify-feature retention config-gc.json --timeout 60s` → 回收 4 blob（2 个 untagged manifest + 2 个随之无引用的 config），v2 依赖的 4 blob 全保留，重启后 v2 manifest 拉取完整。
6. **收尾**：杀 zot（实测真正的服务进程 PID 与 `&` 返回值不同，按 `ss -tlnp` 找真实 PID 杀），5050 释放，`data/` 保留。

## ③ 证据（关键输出摘录）

```text
$ curl http://127.0.0.1:5050/v2/            # zot 起服
GET /v2/ -> 200

$ crane append -f work/alpine-layer.tar.gz --oci-empty-base -t 127.0.0.1:5050/line/alpine:v1 --insecure
02:36:42 existing blob: sha256:e2de96513ba9…        # alpine layer 命中已有 blob（跨 push 去重）
127.0.0.1:5050/line/alpine@sha256:d978230bded0d68a765b20d31dbaba90ac0790d3a1efbe534c342c8c0ffacea0
$ crane append -f work/v2layer.tar -b …:v1 -t …:v2 --insecure
127.0.0.1:5050/line/alpine@sha256:239511b519844c308cdc029d5b182f3e796b76ea633c1d9e4f644a0b477e505d

$ crane digest …:v1 ; crane digest …:v2     # 内容寻址
sha256:d978230b…   vs   sha256:239511b5…    # 不同；crane manifest v1/v2 均拉回（OCI manifest，v2 双 layer）

$ cosign sign-blob --key cosign.key --bundle out.bundle artifact.txt && cosign verify-blob --key cosign.pub --bundle out.bundle artifact.txt
Wrote bundle to file out.bundle / Verified OK (exit 0)
$ cmp -l artifact.txt artifact-tampered.txt → "26 157 117"（1 字节）后 verify-blob → exit 1
  "invalid signature when validating ASN.1 encoded signature"
$ env HTTPS_PROXY=http://127.0.0.1:9 cosign verify-blob … --insecure-ignore-tlog artifact.txt
WARNING: Skipping tlog verification… / Verified OK (exit 0)   # 纯离线验证成立

$ zot verify-feature retention config-gc.json --timeout 60s   # 停服后执行
"no retention policies are configured - garbage collection will run with default settings"
"removed untagged manifest … sha256:d978230b… decision:delete reason:deleteUntagged"
"garbage collected blobs … count:4" / "gc successfully completed for data/line/alpine"
GC 前 8 blob → GC 后 4 blob（v2 的 manifest/config/两 layer 全保留），重启 zot 后 crane manifest v2 完整
```

## ④ 对最终方案的反馈

1. **zot v2.1.22 是 OCI-only 写入**：PUT `application/vnd.docker.distribution.manifest.v2+json` → **415 Unsupported Media Type**（MANIFEST_INVALID）；PUT OCI manifest → 201。上游镜像若只有 docker 格式，进 zot 前必须转 OCI。zot 配置里存在未测开关 `http.compat`（schema 中为 string 数组，见 zot-config.schema.json），疑似 docker media type 兼容入口，**M1 应实测它**，能省掉所有转换管线。
2. **crane push 的 tarball 只认 docker-save 格式**（OCI layout tar 报 `file manifest.json not found in tar`），而 zot 又只收 OCI——两条限制叠加，"pull 成 tar 再 push" 的直觉路线在 zot 上走不通。可行组合：`crane append --oci-empty-base`（本次用法）或 `pull --format=oci` 后自组 blob+manifest 推送。接口设计上建议把"上传镜像到 zot"固定为 **OCI manifest 端到端**。
3. **zot 的 manifest 校验只查引用 blob 的 digest+size 存在性，不解析内容**：我手工 curl 的 manifest 把 config/layer 指反（config 指向 3.8MB layer）也返回 201。语义完整性依赖 GC/scrub 而非写入校验——最终方案若做校验线，要靠 `zot scrub`（`--help` 里确认存在）补位，未测。
4. **GC 触发接口定了**：a) 配置驱动周期 GC（`storage.gc/gcDelay/gcInterval`，缺省即开）；b) **一次性 GC**：`zot verify-feature retention <config> [--gc-interval] [--timeout]`，GC 真执行、免 retention 策略，但**本地存储必须先停服**（文档明确 + 实测端口占用即拒绝）。没有找到在线触发 GC 的 REST API。若 M1 要"删 tag 后立即可回收"，要么接受停服窗口，要么用短 gcInterval 的调度。gcDelay 语义：删除后延迟保护期，本次设 1s 实测有效。
5. **cosign v3 离线语义有坑**：`sign-blob` 的 `--bundle` 是必填（新版 bundle 格式，含 rekor tlog 条目，签名时自动上传 tlog）；`verify-blob --bundle` 时即使 `~/.sigstore` 有 TUF 本地缓存，**仍会发起在线 TUF 刷新**（实测死代理下失败）→ 纯离线验证必须加 `--insecure-ignore-tlog`（丢透明账本审计性）或提前固化 trusted-root（未测）。最终方案若要求"验证零网络"，接口里应显式落 `--insecure-ignore-tlog` 或预置 `--trusted-root`。

## ⑤ 遗留问题

- 未测：`http.compat` 开 docker 兼容后 docker schema2 能否直接 PUT（M1 优先补）。
- 未测：周期 GC 在缺省配置（gcInterval 疑似 24h）下的真实长跑行为；本次只验证了 CLI 一次性 GC。
- 未测：retention policies（按 tag 保留策略）、`zot scrub`、S3 storage driver / 多实例下的 GC 锁。
- 未测：cosign 对 registry 内镜像的签名（`cosign sign`，签名存 registry）与 zot referrer API 关联——本次仅 blob 签名（ask 范围）。
- 拉镜像源依赖 public.ecr.aws（docker.io 双栈均不可达）；若生产机同网络，M1 的镜像供给链要预置好。
- zot `verify-feature retention --timeout 60s` 实际等满 60s 才报 "retention check completed successfully"（GC 本身 0.06s 完成），脚本化时注意该命令的等待行为。
