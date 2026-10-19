# REPORT.md — poc-cas-rust：纯 Rust CAS 客户端闭环（不借助 crane CLI）

环境：Rust 1.99.1 / cargo 1.99.1，ureq 3.4.2（no default features）+ sha2 0.10.9 + serde_json，zot v2.1.22（端口 5190，存储根 `data/` 在本任务目录内），cosign v3.1.3。全部结论来自本会话真实执行；命令原文与输出摘录见③。

## ① 结论

**通过**（ask 四个步骤全部真实执行通过；两处环境层面的适配如实记录在④-6/⑤）。

| ask 验收点 | 结果 |
|---|---|
| ① 调研 Rust OCI 客户端 crate，选型或决定手写 | ✅ 完成（crates.io API 一手数据 + 实际编译/互通测试）；**决定 PoC 手写 + 生产双轨建议**，见④-5 |
| ② 最小 push（blob 上传+manifest PUT）与 pull（digest 校验），推→拉→字节比对 | ✅ 四种上传形态全通；落盘自检 9/9；`cmp` 字节全等；负例 404 错误体可读（exit 1） |
| ③ 100MB 随机 layer：分块/流式上传形态与内存占用 | ✅ 四形态全通；流式形态 RSS 恒定 ~4.1MB（与文件大小无关）；chunks 形态 RSS ≈ 基线+chunk_size；服务端去重生效（4 次不同形态推送同一 blob，落盘始终 101M） |
| ④a cosign 镜像签名（对推上去的镜像）+ 与 blob 签名差异 | ✅ `cosign sign` 对 casr 推的镜像签名成功并存入 registry；verify 通过；同 tag 换内容后 verify 失败（exit 10）；差异清单见④-4 |
| ④b zot referrers API 存在性与返回形态 | ✅ 存在；`GET /v2/<name>/referrers/<digest>` → 200 + OCI image index；六种边界形态 + artifactType 过滤实测，发现**缺 `OCI-Filters-Applied` 头**，见④-3 |
| 产出：工件→OCI 映射约定草案 | ✅ 见④-1/④-2（含确定性实测：同内容跨 tag/跨 repo/重推 digest 全等） |

工程：`poc/poc-cas-rust/casr/`（685 行单文件 `src/main.rs`，依赖树 **25** crates）。结束状态：zot 已杀（pid 483603），端口 5190 已释放（`ss` 复核），`data/`（约 101M）保留。

## ② 步骤

1. **调研**：crates.io API 查 `oci-distribution`（0.11.0，最后发版 2024-03-27）与候选；发现同描述的 `oci-client` 0.18.0（936 万下载，2026-09-18 发版）属 **oras-project**（`https://github.com/oras-project/rust-oci-client`）——即 `oci-distribution` 的更名后继（发布者 thomastaylor312 同人）。对比两者依赖（旧：reqwest 0.12 + jwt；新：reqwest 0.13 + oci-spec 0.10，去 jwt）。下载 ureq 3.4.2 源码包核实 `SendBody` 语义（`File` → Content-Length 磁盘流式；`from_reader` → chunked TE；`&[u8]` → Content-Length 内存）。
2. **工程**：`cargo new casr` → 给 `casr/Cargo.toml` 加空 `[workspace]`；cargo new 曾自动把工程挂进 AF 根 `Cargo.toml` members，**已 `git checkout -- Cargo.toml` 还原**（`git diff` 复核为空）。zot 配置 `zot-config.json`（distSpecVersion 1.1.1 / 127.0.0.1:5190 / rootDirectory ./data / debug 日志），`zot verify` 合法后 `serve`。
3. **实现**：`casr` 五个子命令：`push`（config/layer 两 blob + manifest PUT，含 HEAD 存在性跳过）、`pull`（manifest→config→layers，digest+size 双校验）、`blob-push`/`blob-pull`（单 blob，四种上传形态可选）、`refs`（referrers 查询）。上传形态：`mono`（POST init → PUT ?digest=，body=File）；`single-post`（单请求 POST ?digest=）；`chunked-te`（PATCH 未知长度流式，TE chunked）；`chunks`（N× PATCH + Content-Range，固定 buffer）。构建遇 `ring`（ureq 默认 rustls）C 编译写 /tmp 爆配额 → 改 `default-features = false`（纯 HTTP）后 2.4s 编译通过。
4. **闭环**：小工件（77B 含中文文本）四形态各推一个 tag → 各自拉回 → `cmp` 逐字节比对 + 外部 `sha256sum` 复核；`data/` 下 9 个 blob 文件名==sha256(内容) 全对。负例：拉不存在 tag → 404 + `MANIFEST_UNKNOWN` 错误体，exit 1。
5. **大文件**：`dd if=/dev/urandom` 生成 100MB → 四种形态 `blob-push`（`/usr/bin/time -v` 记 Max RSS）→ `blob-pull` 回来 `cmp`；再以完整工件（`push`，manifest 带 100MB layer）闭环一遍；重复推送验证服务端去重（`du` 前后对照）；另测 chunks@1MB 的 RSS 缩放（100 个 PATCH）。
6. **cosign**：`generate-key-pair`（任务目录内）→ `sign`（v3.1.3 拒绝 `--tlog-upload=false`，改用 `signing-config create` 生成的无 tlog 配置 `no-tlog.json`）→ 观察 registry 内落点（无 `.sig` tag）→ `refs` 查 referrers → `verify --key --insecure-ignore-tlog` → 篡改实验：同 tag 重推不同内容 → verify exit 10。
7. **referrers 边界**：有签名/无签名/不存在 subject/不存在 repo 四类 + `?artifactType=` 过滤（匹配/不匹配）逐一 curl 记录状态码与形态。
8. **oci-client 互通测试**：`/home/pan-ding/casr-research/ociclient-test`（任务目录外的研究 scratch，按环境纪律放 /home/pan-ding 下）`cargo add oci-client@0.18`，最小探针拉取 casr 确定性推送的工件；顺手实测其依赖树规模（186 crates）与 rustc 1.99.1 兼容性（lib 编译通过）。
9. **确定性实验**：加 `--deterministic` 开关（config 去掉 created/name/tag/annotations，manifest 去掉 annotations）→ 同内容推 2 个 tag + 另一 repo + 重复推 → manifest digest 全等；100MB 同样验证。
10. **收尾**：zot 日志错误统计（全 session 仅 1 条 error，为负例探测的预期 404）→ 杀 zot → `ss` 确认 5190 释放、进程退出。

## ③ 证据（命令原文与输出摘录）

### ②-4 闭环（mono 形态）

```text
$ ./casr push http://127.0.0.1:5190 demo/hello v1 work/artifact-small.txt
  [layer] sha256:38a757a63e8a3d0937d0bd7d534b3446e7f6fb06ee480a8ebe893dc94e8e5219 size=77 (hash 80.088µs)
  [config] sha256:4a508c270d13bc6feedddbf095ee797a5dccc20c35d90d2e7756a7f662f543a1 size=300
  [manifest] server=sha256:356d4787e7b9517b5917313fb07a8a7a4d7b9132d7a734d0d734d5587616ad45 local=sha256:356d4787…   # PUT 后服务端回传==本地计算
$ ./casr pull http://127.0.0.1:5190 demo/hello v1 out/pulled-v1
  [get-manifest] ct=application/vnd.oci.image.manifest.v1+json server-digest=sha256:356d4787… local=sha256:356d4787…
$ cmp work/artifact-small.txt out/pulled-v1/content.bin && echo IDENTICAL
cmp: IDENTICAL
# sha256sum 两文件均 38a757a63e8a3d09…

# 四形态各推一 tag 后统一拉回比对：
pull v1: cmp IDENTICAL / pull sp1: cmp IDENTICAL / pull ct1: cmp IDENTICAL / pull ck1: cmp IDENTICAL

# zot 落盘自检（python：blobs/sha256 下 文件名 == sha256(内容)）
blobs checked: ok=9 bad=0

# 负例
$ ./casr pull http://127.0.0.1:5190 demo/hello no-such-tag out/neg
Error: HTTP 404 for …/v2/demo/hello/manifests/no-such-tag body={"errors":[{"code":"MANIFEST_UNKNOWN",…}]}
exit=1
```

### ②-5 100MB（`/usr/bin/time -v`，`Maximum resident set size` 即 Max RSS）

```text
$ dd if=/dev/urandom of=work/rand-100mb.bin bs=1M count=100    # 104857600 B，sha256 d011e1037f87d296…

form=mono        hash 1.99s  upload 214ms  RSS 4152 KB  wall 2.20s   # PUT body=File → Content-Length 磁盘流式
form=chunked-te  hash 1.88s  upload 432ms  RSS 4192 KB  wall 2.31s   # PATCH 202 Range=0-104857599（TE chunked 单请求）
form=chunks@8MB  hash 1.74s  13×PATCH      RSS 12528 KB wall 1.94s   # 服务端 202 回累计 Range（0-8388607 → 0-16777215 → …）
form=single-post hash 1.88s  upload 258ms  RSS 4280 KB  wall 2.14s   # zot 连 100MB 单 POST 也收

$ ./casr blob-pull … big/rand sha256:d011e103… out/pulled-100mb.bin
{"digest":"sha256:d011e103…","size":104857600,"elapsed":"2.406s"}  RSS 4220 KB
$ cmp work/rand-100mb.bin out/pulled-100mb.bin → IDENTICAL

# 完整工件（manifest 带 100MB layer）push RSS 3872 KB；pull 后 cmp IDENTICAL
# 去重：同一 100MB blob 经 4 形态 + 工件 push 共 5 次上传，du -sh data/ 始终 101M
# chunks@1MB：100 个 PATCH，RSS 5196 KB（≈ 基线 4MB + 1MB buffer）
```

### ②-6 cosign（v3.1.3）

```text
$ COSIGN_PASSWORD="" cosign generate-key-pair --output-key-prefix casr-sign
Private key written to casr-sign.key / Public key written to casr-sign.pub

$ cosign sign --key casr-sign.key --tlog-upload=false …        # v3.1.3 拒绝：
Error: --tlog-upload=false is not supported with --signing-config or --use-signing-config…
$ cosign signing-config create --out no-tlog.json              # 产物：rekorTlogConfig:{} tsaConfig:{}（无 tlog 服务）
$ COSIGN_PASSWORD="" cosign sign --key casr-sign.key --yes --allow-insecure-registry --signing-config no-tlog.json 127.0.0.1:5190/demo/hello:v1
Signing artifact... / Pushing signature to: 127.0.0.1:5190/demo/hello     exit=0

$ curl …/v2/demo/hello/tags/list
{"name":"demo/hello","tags":["ck1","ct1","sp1","v1"]}          # 无 sha256-….sig tag → v3 默认 referrers 存储

$ cosign verify --key casr-sign.pub --allow-insecure-registry --insecure-ignore-tlog 127.0.0.1:5190/demo/hello:v1
verify exit=0；verified entries: 1；digest bound: sha256:356d4787e7b9517b…（== casr push 回显的 manifest digest）

# 篡改实验：同 tag v1 重推不同内容
篡改后新 manifest digest: sha256:7524559d995bd3fc…
$ cosign verify … :v1
Error: no signatures found                                     exit=10
```

### ②-7 referrers API（zot v2.1.22，全部 curl 实测）

```text
$ GET /v2/demo/hello/referrers/sha256:356d4787…      → HTTP 200，Content-Type: application/vnd.oci.image.index.v1+json
{"schemaVersion":2,"mediaType":"…image.index.v1+json","manifests":[{ "mediaType":"…image.manifest.v1+json",
 "digest":"sha256:bcce6a6e14bed2d78e969542cd8e35242f200244886680c71dff472b45488b12","size":877,
 "annotations":{"dev.sigstore.bundle.content":"dsse-envelope","dev.sigstore.bundle.predicateType":"https://sigstore.dev/cosign/sign/v1",…},
 "artifactType":"application/vnd.dev.sigstore.bundle.v0.3+json"}]}

① 无任何 referrer 的 subject   → 200 + {"manifests":[]}
② 不存在的 subject digest      → 200 + 空列表（不 404）
③ 不存在的 repo                → 200 + 空列表（不 404）
④ ?artifactType=<sigstore bundle>  → 200 + 只含该条（过滤生效）
⑤ ?artifactType=<不匹配值>     → 200 + 空列表
⑥ ①~⑤ 响应均无 OCI-Filters-Applied 头（④ 过滤生效时也没有）
```

### ②-8 oras oci-client 0.18.0 互通与依赖健康

```text
$ cargo tree --prefix none | sort -u | wc -l
186        # casr（ureq no-TLS）= 25；oci-client 全家桶（tokio/reqwest/rustls）= 186
$ cargo build -j4    # rustc 1.99.1
Finished `dev` profile …    # lib 编译通过；报错仅在我探针的 API 误用

# 探针三次迭代学到的 API 语义：
# 1) ClientConfig 默认 ClientProtocol::Https → 本地 http registry 要显式 Http
# 2) pull(&ref, &auth, accepted_media_types) 第三参为空 vec![] 时：
#    IncompatibleLayerMediaTypeError("application/vnd.af.artifact.layer.v1.raw") —— 拒绝任何未声明 media type；
#    且 manifest.layers 为空时直接 PullNoLayersError（client.rs:993-995）
# 3) 按声明传入后：
pulled: digest=sha256:73cb28704caa48e62205aa297300bda5b80d9e2bdbcda388c3c864da06ca945d layers=1 config.media_type=application/vnd.af.artifact.config.v1+json
#    ↑ 与 casr --deterministic 推送时打印的 digest 逐字一致（三向互通：casr 写 → zot 存 → oci-client 读）
```

### ②-9 确定性

```text
$ casr push --deterministic   # config 只留 {artifactType, contentSize, contentDigest}；manifest 去 annotations
demo/hello:det1  -> sha256:73cb28704caa48e62…
demo/hello:det2  -> sha256:73cb28704caa48e62…   # 同 repo 不同 tag
demo/other:det1  -> sha256:73cb28704caa48e62…   # 另一 repo
demo/hello:det1（重推）-> sha256:73cb28704caa48e62…   # 重复推
big/det:v1（100MB）  -> sha256:ad069564e303daeae…
# 对照（非确定性模式）：同内容重推 v1 → digest 356d4787… 变 28ac017f…（config 时间戳所致）
```

### 服务端佐证（zot.log）

```text
PATCH /v2/demo/hello/blobs/uploads/a5d9a134…  202  headers 无 Content-Length/Content-Range   # chunked-te
PATCH /v2/demo/hello/blobs/uploads/aedd8305…  202  Content-Range:["0-31"] Content-Length:["32"]
PATCH /v2/demo/hello/blobs/uploads/aedd8305…  202  Content-Range:["32-63"] … ["64-76"]        # chunks
全 session "level":"error" 共 1 条：GET index.json of data/no/such（负例 404 的预期日志）
```

## ④ 对方案的反馈 / 决策输入

1. **工件→OCI 映射约定草案（casr 实测版本）**：
   - **工件 = 一个 OCI image manifest**（`schemaVersion:2`，`mediaType: application/vnd.oci.image.manifest.v1+json`）。manifest digest 即工件的不可变内容地址；tag 只是可变别名，"工件版本"应该用 digest 表达。
   - **元数据 = config blob**（自定义 `config.mediaType`，如 `application/vnd.af.artifact.config.v1+json`）。config 与内容一样按 sha256 寻址、一起上传；zot 不校验 config 语义（poc-cas VERIFY 已证），校验责任在客户端与服务端 scrub。
   - **内容 = layers[]**，每层任意 mediaType（草案用 `application/vnd.af.artifact.layer.v1.raw`，zot 接受任意值；但注意④-5 的 oci-client 摩擦）。单文件工件 = 单 layer；多文件 = 多 layer（每 layer 仍是独立 blob，天然共享去重）。
   - **artifactType 字段（OCI 1.1）= 工件类型标识**，是 referrers API `?artifactType=` 过滤键（实测过滤生效）。
   - **签名/附加物 = subject 字段 + referrers API**：cosign v3 签名实际就是这个模型（签名 manifest 带 `subject` 指向被签 manifest，registry 用 referrers index 关联）。我们的工件体系签名/attestation 直接沿用，不必自造。
   - **确定性（M1 应采纳）**：config/manifest 里不得含时间戳、tag 名、推送者等外部性字段——本 PoC 实测：非确定性模式下同内容重推 digest 就变（356d4787→28ac017f），而确定性模式下同内容跨 2 tag/跨 repo/重推 digest 全等（73cb2870）。确定性 = 同内容同地址 = 跨 repo 免重传 + 签名不因重推失效（推论，基于上面两组实测：非确定性重推使 verify 失败、确定性重推 digest 不变）。外部性数据放 tag 名或 referrers 附件。
2. **大工件表达**：100MB 单 layer 单 blob 实测四形态全通；内存不构成约束（客户端流式恒 4MB 级）。`chunks` 形态（PATCH+Content-Range）服务端维护累计 offset（202 响应回 Range），协议上天然支持断点续传/并发分块，但**续传语义本次未实测**。OCI spec 单 blob 上限 5GB（zot 侧未测，见⑤）。
3. **referrers 是工件关联的现成底座**：存在性 200/空列表/未知 subject 都返回 200+空 index（宽厚式，客户端要判 manifests 数组而不是状态码）；过滤生效但**不发 `OCI-Filters-Applied` 头**（OCI spec 的 SHOULD 项，严格客户端别依赖该头判断"是否被过滤"）。
4. **cosign 镜像签名 vs blob 签名（本次与 poc-cas 两次实测的对照）**：
   - blob 签名（`sign-blob`）：对**字节串**签名，签名/证书走 `--bundle` 本地文件，验证方需自行拿到原字节与 bundle；签名不进 registry。
   - 镜像签名（`sign`）：对**manifest digest**（间接对全部字节）签名，签名自动作为新 manifest（`artifactType: application/vnd.dev.sigstore.bundle.v0.3+json`，DSSE 封装）经 `subject` 关联推入 registry，通过 referrers API 发现——**无本地 bundle，registry 即签名库**；验证时 cosign 自行拉 referrers。
   - v3.1.3 行为变化：`--tlog-upload` 已弃用，离线签名用 `signing-config create`（产物 `rekorTlogConfig:{}`）+ `--signing-config`；verify 侧无对应 flag，纯离线仍靠 `--insecure-ignore-tlog`（与 poc-cas 结论一致）。
   - 风险：签名绑 manifest digest——tag 指向的内容一变，签名即"失联"（实测 exit 10 "no signatures found"）。工件体系要明确"签名跟随 digest 而非 tag"。
5. **crate 选型建议（决策输入）**：
   - **`oci-distribution` 0.11.0 勿用**：最后发版 2024-03-27，两年半未动；已更名迁移为 oras-project 的 **`oci-client`**（0.18.0，2026-09-18 发版，936 万下载，发布流程活跃）。
   - **内核已有 tokio/async → 用 `oci-client`**：功能全（auth/referrers push 端/压缩协商），本次实测 lib 在 rustc 1.99.1 下编译通过、与手写客户端对同一工件 digest 互通。代价：186 crates 的依赖树、pull 必须声明 accepted_media_types 且空 layers 报错（对"任意工件"语义是显式声明而非硬阻碍，但接口设计要围绕它）。
   - **内核坚持同步/最小依赖 → 手写薄客户端可行且已被本 PoC 验证**：ureq(no default features)+sha2+serde_json = **25 crates**，685 行实现 push/pull/四上传形态/referrers/digest 校验全功能。wire 层完全可控（本任务的形态测量、内存测量都依赖这一点——用库则被封装隔开）。
   - **TLS 注意**：本环境 ureq 默认特性（rustls→ring）的 C 编译在 /tmp 配额受限下失败；`default-features=false` 裁掉后即过。内核若要 https，需 ring/aws-lc 的构建环境（TMPDIR 重定向 /home/pan-ding/tmp 亦可绕）或选 native-tls 路线——留作⑤。
6. **环境层适配（如实记录）**：①构建 ring 需要 `TMPDIR=/home/pan-ding/tmp`（/tmp 配额满，cc 临时文件写不进）；②bash 在本机持续报 "pwd: 写入错误: 超出磁盘配额"（/tmp 86% 满），不影响命令结果，未处理。

## ⑤ 遗留问题

- **未测：zot 单 blob 大小上限**（OCI spec 建议 5GB）；100MB 只证明了形态与内存，未探边界。
- **未测：断点续传**——PATCH 中断后从服务端累计 Range 续传（协议基础已在，客户端逻辑未写）。
- **未测：https/TLS 下的 casr**（本次 no-TLS 构建仅 http）；rustls provider 在本机的可用构建路径未验证。
- **未测：oci-client 的 push 侧**（只实测了 pull 互通与依赖编译健康）。
- **未测：referrers 分页**（`Link` header，本仓库 referrer 数为 1 触发不了分页）；cosign 大量签名后的 referrers 性能未测。
- **未测：>1 个 layer 的多文件工件**（代码支持，实验没跑）与 zot GC 对 casr 推送工件的回收行为（poc-cas/VERIFY 已覆盖 GC 机制本身）。
- cosign 签名与确定性推送的组合验证未闭环（签名 → 删 tag → 确定性重推 → verify 应通过）：基于已测的两组事实可推定成立，但没有直接跑过，勿当结论引用。
- `/home/pan-ding/casr-research/`（ureq 源码包 + ociclient-test 探针工程）为任务目录外的研究 scratch，未清理，可复用可删。
