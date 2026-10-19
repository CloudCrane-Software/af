# REPORT — poc-rekor：rekor-tiles 私有部署签名+离线验证链

- 任务 id：`poc-rekor`
- 执行者：验证工程师（Agent 工厂底座）
- 日期：2026-10-19
- 仓库：`github.com/sigstore/rekor-tiles` @ `3ef36dadedc6b7ac4c13f52d606477962fcf93e9`（2026-10-16，浅克隆，深度 1）
- 版本串（二进制自报）：`v2.0.0-20261016162638-3ef36dadedc6`
- 任务目录：`af/poc/poc-rekor/`（除 apt 外未写任何系统/AF 其他目录；未 commit；未动 AF 根 Cargo.toml——本任务全程未用 cargo）

---

## ① 结论

**通过。** rekor-tiles（Rekor v2）私有部署"签名→入 log→inclusion proof→断网离线验证→篡改必拒"全链在本机真实跑通，且走通了 **cosign 原生客户端全链**（sign-blob 直传私有 log + verify-blob 断网 `Verified OK`）。要点：

1. **最小私有部署形态比预期轻得多**：rekor-tiles 已迁移到 Tessera 后端，`rekor-server-posix` 二进制 + 一个静态文件服务即为完整部署（**无 MySQL/Trillian/Spanner 依赖**，与旧版 rekor 文档印象完全不同）。
2. **闭环与离线验证全部真实执行**：cosign 签名 trace checkpoint（内容+时间戳+序号）→ hashedrekord v0.0.2 POST 入 log（HTTP 201，logIndex=0）→ 自研验证器在 `sudo unshare -n`（netns 内 curl exit 7，仅 DOWN 状态 lo）下凭"预置 log 公钥+checkpoint+proof+artifact"全链 PASS（exit 0）→ 三组篡改负例全部 exit 2。
3. **与 cosign --trusted-root 完全同构，且已实测**：`cosign sign-blob --signing-config(私有) --trusted-root(私有)` 直传私有 log；`cosign verify-blob --key --bundle --trusted-root` 在断网 netns 下 **Verified OK**（无需 `--insecure-ignore-tlog`，透明账本审计性保住了）。这直接补上了 poc-cas 报告"纯离线验证必须 `--insecure-ignore-tlog` 或预置 `--trusted-root`（未测）"中**未测的那一半**。
4. **任务假设纠偏（诚实记录）**：cosign v3.1.3 的 `generate-key-pair` **没有算法旗标**，只产加密的 ECDSA P-256（`ENCRYPTED SIGSTORE PRIVATE KEY`），"可产 ed25519"不成立；log key 改用 openssl 产 ed25519 PKCS#8 PEM（与官方 tests/testdata 同格式）。但实测 **cosign 加密密钥可直接当 log signer**（`--signer-password`），同构性反而更强。
5. **三个生产级坑（决策输入，均有实测/源码定位）**：
   - 写 API 与 tile 读路径**必须同源**（cosign 会从 log baseUrl 拉 `/checkpoint` 验证）；
   - 私有部署带端口时，`--hostname` 必须设为 baseUrl 剥掉 scheme **和端口**的裸 host（sigstore-go 用 `u.Hostname()` 当 checkpoint origin，源码定位见③）；
   - v2 的 `integratedTime` 恒为 0，**log 不背书时间**，时间信任要靠产物自带签名/RFC3161 TSA/witness。

---

## ② 步骤

1. **环境侦察**：确认 cosign v3.1.3、go1.26.8（构建时 GOTOOLCHAIN 自动切 go1.27.x）、docker 可用但不需要、5230-5239 空闲、/tmp tmpfs 配额小（86% 满，构建曾报 `disk quota exceeded`，改 `TMPDIR/GOCACHE` 到任务目录后成功）。
2. **定位与克隆**：`git clone --depth 1 https://github.com/sigstore/rekor-tiles.git`（exit 0）。读 README/CLIENTS.md/e2e 测试确认：POSIX 后端、hashedrekord v0.0.2、验证用 `pkg/verify`。
3. **构建**：`go build -o bin/rekor-server-posix ./cmd/rekor-server/posix`（GOPROXY=goproxy.cn，成功，33MB）。
4. **密钥（自生成）**：
   - log key：`openssl genpkey -algorithm ed25519` → `keys/log-ed25519-priv.pem`（PKCS#8，与官方测试 PKI 同格式）；
   - cosign 对照实测：`cosign generate-key-pair` → 仅 ECDSA P-256 加密密钥（无算法旗标）；
   - 产物签名 key：`cosign generate-key-pair --output-key-prefix trace-signer`（ECDSA P-256，`COSIGN_PASSWORD=poc-test-pass`）。
5. **起服（端口全部落在 5230-5239）**：
   - 写 API `--http-port=5232`、gRPC `5233`、metrics `5234`；
   - tile 读路径：`python3 -m http.server 5235 --directory data/storage`（tessera 磁盘布局即 HTTP 路径：`/checkpoint`、`/tile/0/000.p/1`、`/tile/entries/000.p/1`）；
   - 同源反代 `bin/proxy`（20 行 Go httputil）挂 `5239`：`/api/v2/*`→5232，其余→5235；
   - 加测实例：cosign 加密密钥当 log signer，端口 5236/5237/5238。
6. **闭环**：构造 trace checkpoint（内容+`timestamp_utc`+`seq: 42`+payload sha256）→ `cosign sign-blob --signing-config keys/signing-config-empty.json --bundle`（空 signing-config=零外部服务，纯本地签名）→ 组装 hashedRekordRequestV002 JSON → POST `/api/v2/log/entries`（**201**）→ 拆分 evidence：`checkpoint.txt`/`proof.json`/`canonicalized_body.b64`。
7. **离线验证器**：`verify/main.go`（Go，复用 rekor-tiles `pkg/verify`/`pkg/note`/`pkg/types/hashedrekord` + `transparency-dev/merkle`，replace 指向本地克隆）→ `bin/pocverify`，输入严格为"公钥+checkpoint+proof+body+artifact+产物公钥"。
8. **断网验证**：`sudo unshare -n bash -c 'curl(证明无网)+pocverify'`。
9. **篡改负例**：artifact 改 seq / checkpoint 改树大小 / body 改 digest 字符，三种全部必须失败。
10. **cosign 原生链**：`cosign signing-config create --rekor="url=http://127.0.0.1:5239,api-version=2,..."` → `cosign trusted-root create --rekor="url=...,public-key=log公钥,origin=127.0.0.1" --no-default-*` → `sign-blob --signing-config --trusted-root --bundle` → 断网 `verify-blob --key pub --bundle --trusted-root`。
11. **行为加测**：重启持久性（checkpoint 原样）、重复提交行为（默认配置重启后 201 入新 index）。
12. **清理**：杀全部常驻进程，`ss -tln` 确认 5230-5239 全部释放（残留的 18898 是本任务开始前已存在的无关进程）。

---

## ③ 证据（命令原文与输出摘录）

### 3.1 构建与起服

```
$ cd rekor-tiles && export GOPROXY=https://goproxy.cn TMPDIR=<task>/.tmp GOCACHE=<task>/.gocache
$ go build -o ../bin/rekor-server-posix ./cmd/rekor-server/posix   # 首次在 /tmp 报 disk quota exceeded，转任务目录后
build_exit=0; ls -la ../bin/ → rekor-server-posix (33,206,834 B)

$ bin/rekor-server-posix serve --http-address=127.0.0.1 --http-port=5232 --grpc-port=5233 \
    --http-metrics-port=5234 --hostname=rekor-poc.local --storage-dir=$PWD/data/storage \
    --signer-filepath=$PWD/keys/log-ed25519-priv.pem --checkpoint-interval=2s
level=INFO msg="starting Tessera sequencer" / "Starting gRPC server" address=127.0.0.1:5233
$ curl -s http://127.0.0.1:5232/healthz → {"status":"SERVING"}
$ curl -s http://127.0.0.1:5235/checkpoint →
  rekor-poc.local / 0 / 47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU= / （空行）/ — rekor-poc.local <sig>
```

### 3.2 密钥实测（任务假设纠偏）

```
$ cosign generate-key-pair --help        # Flags 仅 --kms / --output-key-prefix，无 key-algorithm
$ COSIGN_PASSWORD=poc-test-pass cosign generate-key-pair --output-key-prefix trace-signer
→ trace-signer.key 头：-----BEGIN ENCRYPTED SIGSTORE PRIVATE KEY-----（scrypt）
→ trace-signer.pub 头：MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE…（=ECDSA P-256 SPKI）
$ openssl genpkey -algorithm ed25519 -out keys/log-ed25519-priv.pem   # BEGIN PRIVATE KEY（PKCS#8）
```

### 3.3 签名与入 log（闭环正例）

```
$ cosign sign-blob --signing-config keys/signing-config-empty.json --bundle evidence/trace-checkpoint.bundle.json \
    --key keys/cosign-gen/trace-signer.key evidence/trace-checkpoint.txt   # exit 0，零外部服务
bundle: messageSignature.signature=MEYCIQDuFPwD…, messageDigest=nUCCLK4VnMHtsoIy6/7AVsFCrXnZh9xhHli30lYj2vo=

$ curl -s -X POST -H 'Content-Type: application/json' --data @evidence/create-entry-request.json \
    http://127.0.0.1:5232/api/v2/log/entries -w '%{http_code}'   → http_code=201
tle.json 摘录：logIndex="0"；logId.keyId="elhv2rKrZIN+UcBuleISelxN2ZiXEr2SDW5vkLa9GwM="；
kindVersion=hashedrekord/0.0.2；integratedTime="0"；inclusionProof.treeSize="1"、hashes=[]；
checkpoint.envelope="rekor-poc.local\n1\n7pzSY6ixM+zONo49bnd7wd7OILRxNJr7GwQkTtlJ3x8=\n\n— rekor-poc.local elhv2sX2…"
（注意：checkpoint 签名行前 4 字节 keyhash 与 logId.keyId 前缀一致 → C2SP key-id 体系自洽）

读路径（tessera 布局即静态路径）：
$ curl -s http://127.0.0.1:5235/tile/0/000.p/1 | xxd → ee9cd263…（=checkpoint 根哈希解码）
$ curl -s http://127.0.0.1:5235/tile/entries/000.p/1 | head -c 120 →
  {"apiVersion":"0.0.2","kind":"hashedrekord",…,"digest":"nUCCLK4VnMHtsoIy6/7AVsFCrXnZh9xhHli30lYj2vo="}
```

### 3.4 断网离线验证（④，通过）

```
$ sudo unshare -n bash -c '
    curl -s --max-time 3 http://127.0.0.1:5232/healthz; echo curl_exit=$?   # curl_exit=7（无网，lo DOWN）
    ./bin/pocverify -log-pub keys/log-ed25519-pub.pem -origin rekor-poc.local \
      -checkpoint evidence/checkpoint.txt -proof evidence/proof.json \
      -body evidence/canonicalized_body.b64 -artifact evidence/trace-checkpoint.txt \
      -artifact-pub keys/cosign-gen/trace-signer.pub'
PASS inputs loaded
PASS checkpoint signature (origin=rekor-poc.local size=1 root=7pzSY6ixM+zONo49bnd7wd7OILRxNJr7GwQkTtlJ3x8=)
PASS inclusion proof (logIndex=0 treeSize=1)
PASS artifact digest matches logged entry digest
PASS artifact ECDSA signature verifies
PASS reconstructed entry (from artifact alone) is provably included in the log
RESULT: VERIFIED — artifact (trace checkpoint) is bound into the transparency log
offline_verify_exit=0

（cosign 原生侧同料断网验签：）
$ cosign verify-blob --key keys/cosign-gen/trace-signer.pub --signature evidence/trace-checkpoint.sig \
    --insecure-ignore-tlog evidence/trace-checkpoint.txt   # 断网下 Verified OK / exit 0
```

### 3.5 篡改负例（⑤，全部正确失败）

```
N1 改 artifact 内容（seq: 42→43）：
FAIL at artifact digest binding: artifact sha256 tNV+Fnp1… != logged digest nUCCLK4V…（改动在入 log 之后）exit=2
N2 改 log checkpoint（树大小 1→2）：
FAIL at checkpoint signature verification: invalid signature for key rekor-poc.local+7a586fda  exit=2
N3 改 entry body 内 digest 字符（v0→A，保持 JSON/base64 合法）：
FAIL at inclusion proof verification: calculated root: [198 37 169 …] does not match expected root: [238 156 210 …] exit=2
```

### 3.6 cosign 原生全链（同构性，通过）

```
$ cosign signing-config create --rekor="url=http://127.0.0.1:5239,api-version=2,start-time=2025-01-01T00:00:00Z,operator=poc" \
    --rekor-config="ANY" --out keys/signing-config-private.json
$ cosign trusted-root create \
    --rekor="url=http://127.0.0.1:5239,public-key=$PWD/keys/log-ed25519-pub.pem,start-time=2025-01-01T00:00:00Z,origin=127.0.0.1" \
    --no-default-ctfe --no-default-fulcio --no-default-rekor --no-default-tsa --out keys/trusted-root-private.json
→ tlogs[0].logId.keyId=ImgxNmDMvbpAV14nHARcfcM6SfHJWBqQnHpNaNgwYb4=（与服务端 TLE 的 logId 完全一致）

$ cosign sign-blob --signing-config keys/signing-config-private.json --trusted-root keys/trusted-root-private.json \
    --bundle evidence/trace-checkpoint-2.bundle.json --key keys/cosign-gen/trace-signer.key evidence/trace-checkpoint-2.txt
→ "Wrote bundle to file…"；bundle.verificationMaterial.tlogEntries length=1；
  tlogEntries[0].logId=ImgxNmDM…、kind=hashedrekord/0.0.2、checkpoint 首行 origin="127.0.0.1"
  （proxy.log 证据：仅 "POST /api/v2/log/entries -> write"，TLE 自包含，签名端不再拉 tile）

$ sudo unshare -n bash -c 'cosign verify-blob --key keys/cosign-gen/trace-signer.pub \
    --bundle evidence/trace-checkpoint-2.bundle.json --trusted-root keys/trusted-root-private.json \
    evidence/trace-checkpoint-2.txt'
→ Verified OK / exit=0
（同命令验篡改件 evidence/tampered/artifact-native.txt：invalid signature… / exit=1）
```

### 3.7 行为加测

```
重启持久性：kill 后同 storage-dir 重启，0 ERROR，data/storage/checkpoint 原样（origin/size/根哈希不变）。
重复提交（默认 --persistent-antispam=false）：重启前 409 + "an equivalent entry already exists … index 0"；
重启后同 entry → HTTP 201、logIndex="1"、treeSize="2"（内存去重丢失，同条目占两个 index）。
cosign 加密密钥当 log signer：--signer-filepath trace-signer.key --signer-password=poc-test-pass
（端口 5236/5237/5238，origin=rekor-poc-cosign.local）→ POST 201，pocverify 全链 PASS（ECDSA checkpoint 验签通过）。
```

### 3.8 源码定位（坑的根据）

- `pkg/types/hashedrekord/hashedrekord.go:54-58`：纯 Ed25519（PKIX_ED25519）无预哈希被显式拒绝；默认允许集见 `internal/algorithmregistry/algorithmregistry.go`（RSA×3 / ECDSA P256/384/521 / **PKIX_ED25519_PH**）。
- `internal/signerverifier/`（抄自 rekor `pkg/signer/file.go`）：`cryptoutils.UnmarshalPEMToPrivateKey(data, StaticPasswordFunc(keyPass))` → 支持 cosign 加密私钥 + `--signer-password`。
- `cmd/rekor-server/posix/app/serve.go:119`：`--witness-policy-path` 为空即跳过 witness（最小部署无外部依赖）。
- **`sigstore-go@v1.3.0/pkg/verify/tlog.go`（cosign v3.1.3 所用）**：
  ```go
  u, err := url.Parse(tlogVerifier.BaseURL)
  noteVerifier, err := note.NewNoteVerifier(u.Hostname(), *verifier)   // ← origin=去 scheme 且去端口的 host
  ```
  这解释了 `--hostname=127.0.0.1:5239` 时 cosign 报 "note has no verifiable signatures"、改为 `--hostname=127.0.0.1` 后全链通过。
- cosign 签名端 TLE 校验同样自包含：`reconstructV2EntryHash(...)` + `VerifyLogEntryWithHash(...)`（与我们验证器 4c 步骤同构）。

---

## ④ 对方案的反馈/决策输入

### 4.1 trace 审计链密钥基建形态（任务产出）

**两类密钥、两种信任分发、三条时间路径：**

| 角色 | 算法/格式 | 住哪 | 生成/保护 | 轮换 |
|---|---|---|---|---|
| **log key**（签 checkpoint/entry tiles） | ed25519 PKCS#8 PEM（或 cosign 加密密钥+password，实测可用；云后端走 KMS/Tink） | rekor server 主机文件系统（`--signer-filepath`） | 明文 PEM 或 scrypt 加密+`--signer-password` | **不能原地换**：checkpoint key-id=SHA256(origin‖0x0A‖alg‖pub)[:4] 绑死 origin+key；换 key=开新 shard（新 origin+新 key），老 shard 冻结只读继续可验（官方分片 playbook：sigstore/public-good-instance `tiles-tlog-sharding.md`；公网实例约每 6 个月分片一次） |
| **producer key**（签 trace checkpoint 产物） | ECDSA P-256（cosign 默认）/RSA/Ed25519**ph**；纯 Ed25519 不可用于 hashedrekord | 产物签名方（cosign 文件密钥/KMS/sk） | cosign 加密私钥（scrypt） | 自由轮换：每条 entry 内嵌 verifier 公钥，老 entry 用老钥验，TrustedRoot 保留历史 key+validity 窗口即可 |

**信任分发 = cosign v3 原生的两份 JSON（与公网基础设施完全同构）：**
- 写侧 `SigningConfig`（`application/vnd.dev.sigstore.signingconfig.v0.2+json`）：`rekorTlogUrls` 指私有 log URL（majorApiVersion=2）；
- 验侧 `TrustedRoot`（`trustedroot+json;version=0.1`）：`tlogs[]` 放 log 公钥+baseUrl；客户端按 **C2SP checkpoint key-id** 选 log（本 poc 实测 logId 全等）。
- `cosign --trusted-root` 模式即此：**私有部署不是"绕过 cosign"，而是把 TUF 公共信任根换成自产 SigningConfig+TrustedRoot 两个文件**，cosign 行为不变。cosign v3.1.3 对 v2 log 的离线 tlog 验证已原生支持（本 poc 实证），poc-cas 的 `--insecure-ignore-tlog` 折衷可以退役。

**部署拓扑（实测形态）**：`rekor-server-posix`（写 API+gRPC+metrics）与 tile 静态文件**必须同一 origin 对外**（生产=nginx/反代合并；本 poc=20 行 Go 反代 5239）。tiles 是普通文件，读路径无鉴权可上 CDN；唯一可变面是写 API。

### 4.2 时间戳与序号的真相（对 trace 方案直接有效）

- `integratedTime` 恒为 `0`（实测），**Rekor v2 不提供服务器背书的时间**。trace checkpoint 的"时间戳+序号"要分层：
  1. 产物内容自带 `seq`+`timestamp`+payload hash，由 producer key 签名——这是业务序号的信任来源；
  2. log 给的是**全局单调 index + inclusion proof**（防篡改/防重排，不给时间）；
  3. 需要第三方时间的离线证明时：RFC3161 TSA 或 witness cosignature（`--witness-policy-path` 可选挂载，本次未启用，官方计划同步 co-signed checkpoint 兼作时间戳）。

### 4.3 生产清单（全部有实测依据）

1. **必须 `--persistent-antispam=true`**：默认内存去重，重启后同一条目重新入 log 拿新 index（实测 201），破坏"一产物一 index"审计假设。
2. **`--hostname` = 客户端 baseUrl 的 `url.Hostname()`**（去 scheme、去端口）：带端口的私有地址（host:5239）会导致 cosign 验签 "note has no verifiable signatures"。生产用域名默认 443 无此坑，但测试/预发带端口必踩。
3. 写 API 与 tiles 同源反代；gRPC/metrics 端口不对外。
4. TLS 旗标现成（`--http-tls-cert-file` 等），内网 poc 未配；生产必须上。
5. cosign 侧产物算法选 ECDSA P-256 最省事（cosign 默认即此）；若强推 ed25519 产物需自实现 Ed25519ph 签发器，成本不值。
6. 构建环境：Go ≥1.27（本机 GOTOOLCHAIN 自动拉 go1.27.2，走 goproxy.cn 正常）；**/tmp tmpfs 配额小，GO 构建必须 `TMPDIR/GOCACHE` 指到大盘目录**（本机 /tmp 已 86% 满，首次构建直接 quota 报错）。

---

## ⑤ 遗留（未做/未测，如实列出）

1. **witness 链路未部署**：omniwitness + witness policy（M-of-N cosignature）只读了文档；同步 witnessing（checkpoint 兼时间戳）未实测。
2. **RFC3161 TSA 未实测**：私有 TSA（如 当 RFC3161 服务）签发+离线验证未做。
3. **Ed25519ph 产物签名未实测**（无现成签发器；ECDSA P-256 已闭环）。
4. **gRPC 写 API 未测**（HTTP 已闭环，gRPC 仅确认监听）。
5. **TLS 形态未测**；反代是 20 行 Go demo，生产应换 nginx/网关并做 HSTS。
6. **性能未压**（官方 e2e 有 260 条并发写入用例，本 poc 只做功能链）；posix 后端在高并发下的原子序节性能没有数据。
7. cosign v3.1.3 对**旧版 trusted-root v0.1 的 origin 推导**是当前行为，未来版本若改（v0.2 有 checkpointKeyId/origin 字段）需复验；rekor-tiles 迭代快（本次 commit 为 2026-10-16），**M1 落地必须 pin commit**。
8. 本机观察：/tmp tmpfs 已 86% 满（6.4G/7.5G），多个会话共享；建议工厂层面统一 `TMPDIR/GOCACHE/GOPATH` 指向大盘（本次只在任务目录内自救）。

---

## 附：任务目录产物清单

```
af/poc/poc-rekor/
├── REPORT.md                        # 本报告
├── bin/                             # rekor-server-posix / pocverify / proxy
├── rekor-tiles/                     # 浅克隆 @3ef36dad（构建缓存已置 .gocache/.tmp）
├── keys/
│   ├── log-ed25519-priv.pem         # log key（openssl ed25519，PKCS#8）
│   ├── log-ed25519-pub.pem
│   ├── cosign-gen/trace-signer.{key,pub}   # cosign ECDSA P-256（scrypt 加密）
│   ├── signing-config-empty.json    #   零外部服务签名配置
│   ├── signing-config-private.json  #   指向私有 rekor v2
│   └── trusted-root-private.json    #   私有信任根（log 公钥）
├── evidence/                        # trace checkpoint 产物/签名/TLE/checkpoint/proof/负例件
├── data/storage*/                   # tessera posix 存储（三套：主链/加测/原生链）
├── verify/                          # 离线验证器与同源反代（Go 源码）
└── server*.log, proxy.log, static-server*.log
```

常驻进程已全部终止，5230-5239 端口已确认释放（`ss -tln` 为空）。
