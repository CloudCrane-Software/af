# VERIFY.md — poc-cas 三个"未实测"项补充验证（在线周期 GC 长跑 / zot scrub / http.compat）

环境：zot v2.1.22（`/home/pan-ding/workspace/临时工作区/temp2/af/bin/zot`）+ crane 0.22.1，纯二进制。全部结论来自真实执行，日志存 `verify-logs/`，配置与脚本存 `verify/`，存储根均为新建（`verify/data-a`、`data-b`、`data-b2`、`data-b3`、`data-c`、`data-nocompat`、`data-a-mgmt`），未改动 poc-cas 既有文件。所有 zot 进程已杀、端口 5151-5156 全部释放（`ss -tlnp` 复核）。

## ① 结论

| 项 | 结论 | 一句话 |
|---|---|---|
| A 在线周期 GC 长跑 | **通过** | 101 周期（≈28 min 实际墙钟）时间加速长跑：95 个删除的 tag 全部被**在线** GC 回收（0 漏收），6 个长寿 tag 0 误删且全部拉回，终态 blob 集合与理论值**精确相等**（18=18），全程 0 条 error 日志；跑完停服 scrub 6/6 tag "ok" |
| B zot scrub 语义 | **通过（有边界）** | 三类损坏全部检出；且 scrub **会解析 config blob 内容**（digest 完全一致的 config/layer 互换也能检出"invalid server config"）——"写入校验不解析内容"的缺口确实由 scrub 补上；边界：内容"合法但错误"的 config（结构合法、身份不对）检不出；**检出时退出码仍为 0** |
| C http.compat | **通过** | `http.compat: ["docker2s2"]` 使主端口直接接受 docker schema2 manifest PUT（201，此前 415）、docker manifest list 也 201；crane 全链路互通；**不是独立端口、也不做格式转换**——docker manifest 原样落盘（逐字节一致、保留 docker digest），OCI/docker 两种格式同 repo 共存 |

## ② 步骤

### A. 在线周期 GC 长跑（时间加速法）
1. `verify/config-a.json`：`storage: {gc: true, gcDelay: "1s", gcInterval: "2s"}`，rootDirectory=verify/data-a，端口 5151，`zot verify` 合法 → `zot serve`。
2. 冒烟定位机制：手工 push→delete 一次，发现回收在线发生（delete 后 ~2s）；再做 60s 空闲观察——**0 次 GC 执行**，证明 GC 是"仓库变更触发的调度任务"而非墙钟定时器（见⑤-1）。
3. `verify/gc-loop.sh` 跑 101 个周期（c0..c100）：每周期生成唯一小 layer（内容含周期号+纳秒时间戳）→ `crane append --oci-empty-base` 推 tag → 校验 manifest/config/layer 3 个 blob 落盘 → `crane delete` 删 tag（c0/c20/c40/c60/c80/c100 共 6 个为长寿 tag **不删**）→ 轮询等该周期的 manifest+layer blob 消失（上限 45s，1s 步进），记录回收时延。机器可读状态在 `verify/gc-state.tsv`，逐周期结果在 `verify-logs/gc-loop-result.log`。
4. 终态：空闲 30s 后 `verify/final-a-checks.sh` → 6 个长寿 tag 逐个 `crane pull`（digest 校验下载）；`crane manifest` 求出 6 tag 可达 blob 集合并集，与磁盘 blobs 目录**逐文件 diff**。
5. A4：curl 探测 `/v2/_zot/ext/retention`（`/v2/_zot/ext.retention` 同测）GET/POST 于带 `extensions.mgmt` 的 5152 与裸 5151；`strings zot | grep '/v2/_zot'` 枚举二进制路由。
6. A5：杀 5151 → `zot scrub verify/config-a.json`。

### B. scrub 语义
1. `verify/config-b.json`（gc:false，端口 5154，rootDirectory=verify/data-b）→ `crane append` 推 1 个完整 OCI 镜像（manifest db213b… / config ca7b18… / layer 61bc01…）→ 停服 → `zot scrub verify/config-b.json` 基线（先看 `--help`：仅 `scrub <config>`，无额外 flag）。
2. 备份 `data-b.pristine`，每类损坏后恢复再注入下一类，全部停服运行 `zot scrub`：
   - ① `rm` 被引用的 layer blob；
   - ② layer blob 内容翻转 1 字节（文件名不变）；
   - ③a manifest 的 config.digest 改为全 0 不存在 digest（原地改，文件名不变）；
   - ③b-inplace：config↔layer 引用互换（两个 blob 都存在，size 改为对端真实值）；
   - ③b-consistent：同 ③b 互换后**重新内容寻址**——manifest 写入新 digest 文件名、删旧文件、更新 index.json，磁盘上 digest 全自洽、仅语义错（config 位上是 layer）；
   - ③c：config 指向另一个**结构合法但身份错误**的 OCI config blob（同样 digest 全自洽）。
3. 追加在线 scrub：`verify/config-b2-online-scrub.json`（`extensions.scrub: {enable: true}`）起服推镜像→停服篡改 blob→重启，观察启动时的 scrub 任务；`verify/config-b3-online-scrub-5s.json` 试 `interval: "5s"`。

### C. http.compat
1. 查 schema（本地 `zot-config.schema.json` 与 `zot schema` 实时输出一致）：`http.compat` = string 数组，无 enum；`strings zot` 中存在取值 **`docker2s2`**。
2. `verify/config-c.json`：主端口 5153 + `compat: ["docker2s2"]` 起服；curl 手工走 registry API：上传 config/layer blob（201），组 docker schema2 manifest（mediaType `application/vnd.docker.distribution.manifest.v2+json`）PUT。
3. 分别用 docker Accept / OCI-only Accept / 无 Accept GET 该 tag；按原 docker digest GET；`crane manifest/digest/pull/ls` 全链路；再 PUT docker manifest list、PUT OCI manifest 验证共存。
4. 负对照组：`verify/config-nocompat.json`（无 compat，端口 5156）同一份 blob+manifest 重放 PUT。

## ③ 证据（命令输出摘录；完整日志见括号路径）

### A
```text
# 101 周期统计（verify/gc-state.tsv + verify-logs/gc-loop-result.log）
cycles total=101  long-lived kept=6  deleted+polled=95
on_disk=0 的周期     : 0      # 每周期 3 blob 全部落盘
reclaimed            : 95/95  # 0 个 NOT RECLAIMED
reclaim latency (ms) : min=1019 median=17190 mean=17357 max=31372
# 服务端 GC 日志汇总（verify-logs/zot-a.log）
gc runs with collections: 103, total blobs collected: 291   # 95周期×3 + 冒烟/probe×3 = 291，精确吻合
"removed untagged manifest" 次数: 97                        # 95 + 冒烟 + probe
"level":"error" 行数: 0；warn 仅 2 条（swagger/gqlplayground 未编入本 binary，与 GC 无关）

# 长寿 tag 拉回 + 终态对账（verify-logs/gc-final-checks.log）
PULL c0 OK / c20 OK / c40 OK / c60 OK / c80 OK / c100 OK     # crane pull，digest 校验下载
expected(unique reachable from 6 tags): 18
on-disk:                                18
BLOB SET EXACT MATCH                                         # 逐文件 diff 为空

# A5 交叉验证（verify-logs/scrub-after-gc.log，exit=0）
REPOSITORY  TAG   STATUS
gclong/img  c0    ok     c20 ok   c40 ok   c60 ok   c80 ok   c100 ok
```

### A4（在线 retention REST API）
```text
$ curl -X POST http://127.0.0.1:5152/v2/_zot/ext/retention -d '{}'   # mgmt 开着的 5152
-> 404 page not found     # GET/POST × /v2/_zot/ext/retention 与 /v2/_zot/ext.retention 全部 404（5151 同样 404）
$ strings zot | grep -oE '/v2/_zot[a-z/_.]*' | sort -u
/v2/_zot/ext/cosign/v  /v2/_zot/ext/mgmt  /v2/_zot/ext/notationfailed
/v2/_zot/ext/search    /v2/_zot/ext/userprefs  /v2/_zot/ext/userprefsfailed  /v2/_zot/pprof/...
# → 二进制中根本不存在 retention 路由
$ 起服日志（verify-logs/zot-mgmt.log）
"mgmt extensions configuration option has been made redundant and will be ignored."
"skip enabling the mgmt route as the config prerequisites are not met"   # mgmt 路由需先配身份验证
```

### B
```text
$ zot scrub config-b.json                                       # 基线（scrub-baseline.log，exit=0）
REPOSITORY  TAG  STATUS   AFFECTED BLOB  ERROR
scrub/repo  v1   ok

# ① 删除被引用 blob（scrub-corrupt1.log，exit=0）
scrub/repo v1 affected 61bc01…  blob not found: storage object or path is missing: …/61bc01…

# ② blob 翻转 1 字节（scrub-corrupt2.log，exit=0）
scrub/repo v1 affected 61bc01…  bad blob digest

# ③a manifest 指向不存在 digest（scrub-corrupt3a.log，exit=0）
scrub/repo v1 affected db213b…  bad blob digest          # 因原地改文件→不再匹配自身文件名，属附带检出

# ③b-consistent digest 全自洽的 config/layer 互换（scrub-corrupt3b-consistent.log，exit=0）
新 manifest digest 6133e958…（文件已重命名、index.json 已更新，磁盘自洽）
scrub/repo v1 affected 61bc01…  invalid server config   # ← scrub 解析了 config 位上的内容（是 layer 不是 config JSON）

# ③c 合法但错误的 config（scrub-corrupt3c.log，exit=0）
scrub/repo v1 ok                                          # 结构合法即放行，身份不校验

# 在线 scrub（verify-logs/zot-b2.log，extensions.scrub enable，服务运行中）
"executing scrub to check manifest/blob integrity for …/scrub/online"
"blobs/manifest affected" image=scrub/online tag=v1 status=affected affected blob=61bc01… error="bad blob digest"
# interval 调短被钳制（verify-logs/zot-b3.log）
"scrub interval set to too-short interval < 2h, changing scrub duration to 2 hour"
```

### C
```text
# compat 端口 5153（verify-logs/zot-c.log）
$ curl -X PUT …/v2/compat/test/manifests/docker-v1 -H 'Content-Type: application/vnd.docker.distribution.manifest.v2+json' --data-binary @dmanifest.json
-> 201                                                    # 此前无 compat 时为 415
$ curl -X PUT …/manifests/docker-list -H 'Content-Type: application/vnd.docker.distribution.manifest.list.v2+json'
-> 201
$ curl -H 'Accept: application/vnd.docker.distribution.manifest.v2+json' …/manifests/docker-v1
Content-Type: application/vnd.docker.distribution.manifest.v2+json
Docker-Content-Digest: sha256:da480b74919725eb805f33dc77f9e4e006b938d46eea969c982ec5c17bbfcbe0
$ curl -H 'Accept: application/vnd.oci.image.manifest.v1+json' …/manifests/docker-v1
-> 仍返回 docker manifest 原文（不转换、不按 Accept 协商）
$ crane manifest/digest/pull/ls 127.0.0.1:5153/compat/test:docker-v1 --insecure
digest=sha256:da480b74…（docker digest）；pull 产出 docker-save 格式 tar（manifest.json）
# 落盘逐字节验证
PUT bytes sha256: da480b74919725eb == disk blob sha256 → byte-identical: True
index.json: {"mediaType":"…docker…manifest.v2+json","digest":"sha256:da480b74…","ref.name":"docker-v1"}
# 负对照组（verify-logs/zot-nocompat.log，端口 5156）
$ 同一份 docker manifest PUT → HTTP 415 {"errors":[{"code":"MANIFEST_INVALID",…}]}
```

## ④ 对原 PoC 结论的修正

1. **"没有找到在线触发 GC 的 REST API"（RESULT.md ④-4）——证实成立，且证据升级**：v2.1.22 二进制中根本不存在 retention 路由（全部 `_zot` 路由 = cosign/mgmt/notation/search/userprefs/pprof），配了 `extensions.mgmt` 也 404；mgmt 路由本身存在但"config prerequisites are not met"（需身份验证配置），且与 retention 无关。**该项无法推翻，M1 不要在"在线 REST 触发 GC/retention"上做设计假设。**
2. **"周期 GC 在缺省配置下的真实长跑行为未测"（⑤）——已补测并通过**：在线周期 GC 101 周期 0 漏收 0 误删，M1 可以放心用"gc:true + 短 gcDelay/gcInterval"替代"停服跑 `verify-feature retention`"作为常规回收手段。
3. **"http.compat 疑似 docker 兼容入口"（④-1）——猜测证实，但两处修正**：a) compat 不是独立兼容端口，是主 http 监听上的开关（任务里"配 compat 端口"的前提不成立）；b) v2.1.22 的 docker2s2 **不做 docker→OCI 转换**，manifest 原样落盘并保留 docker digest。因此 RESULT.md ④-1 的"上游 docker 格式镜像进 zot 前必须转 OCI"结论**可撤销**：开 compat 即可直接 PUT，代价是仓库内并存两种 media type（见⑤-6）。
4. **"语义完整性依赖 GC/scrub 补位，scrub 未测"（④-3）——已补测**：scrub 不只查 digest+size 存在性，还校验 blob 内容对文件名的 digest（②）并**解析 config blob 内容**（③b-consistent 的"invalid server config"）。缺口被补上；唯一残余盲区是"结构合法但身份错误的替换"（③c），这类要靠签名/cosign 之类上游完整性手段，scrub 天然管不了。
5. **gcInterval 语义修正**（RESULT.md ⑤"gcInterval 疑似 24h"）：它不是"每 24h 无条件跑一次"的墙钟定时器——实测空闲仓库 60s 内 0 次 GC 执行；GC 任务由仓库变更触发，gcInterval 只是调度下限，实际回收时延受调度器抖动支配（本配置下 1s~31s，中位数 17s）。

## ⑤ 新发现

1. **在线 GC 的真实时延画像**：gcDelay=1s/gcInterval=2s 配置下，删除 tag 后 blob 回收耗时 min 1.0s / median 17.2s / max 31.4s（95 样本）。"删 tag 后立即可回收"不成立，正确预期是"数十秒内回收"；上限看起来由调度器 max-delay 类机制决定（~30s），把 gcInterval 调得更小也不会更快。若 M1 有"删除后立刻复用空间"的诉求，需要另想（如同步删除接口语义、或接受最终一致）。
2. **scrub 的退出码恒为 0**（包括检出损坏时）——自动化必须解析表格输出（STATUS=affected 行）或 scrub extension 的 warn 日志，不能依赖 exit code。
3. **scrub 可在线运行**：`extensions.scrub: {enable: true, interval: <duration>}` 起服即跑全量 scrub 并把损坏以 warn（"blobs/manifest affected"）打进日志，周期默认 24h、**可配置但有 2h 下限**（设 5s 被 log 明示改为 2h）。在线 GC + 在线 scrub 组合可以做到"回收与完整性巡检都不停服"。
4. **GC 计数守恒**：103 次 GC 共回收 291 blob、97 个 untagged manifest，与"95 周期×3 blob + 冒烟/probe 各 3 blob、95+2 个 untagged"精确吻合——GC 没有多删也没有少删，账目闭环。
5. **mgmt 扩展配置已冗余**（"made redundant and will be ignored"），路由级开关改为"检测到身份验证配置才挂载"——配置 compat/巡检类功能时不要照抄旧文档的 `extensions.mgmt.enable` 写法。
6. **compat 模式的互通性代价**：docker manifest 原样存储后，即使客户端 Accept 只声明 OCI，zot 也原样返回 docker manifest（不协商）——严格的 OCI-only 客户端会拿到未请求的 mediaType；tag 的 digest 也是 docker digest。crane 实测无碍（它同时Accept 两种）。M1 若开 compat，接口文档要写明"仓库内 media type 不归一"，cosign 对 docker 格式镜像的签名/referrers 关联未测，纳入 M1 待办。
7. `zot schema` 实时输出与本地 `zot-config.schema.json` 一致（compat 均为 string 数组、无 enum），取值合法性只能靠 binary strings + 实跑确认。

### 附：产物清单
- 配置：`verify/config-a.json`（GC 长跑）、`config-a-mgmt.json`、`config-b.json`（scrub）、`config-b2-online-scrub.json`、`config-b3-online-scrub-5s.json`、`config-c.json`（compat）、`config-nocompat.json`
- 脚本：`verify/gc-loop.sh`、`verify/final-a-checks.sh`；状态：`verify/gc-state.tsv`、`verify/expected-blobs.txt`、`verify/ondisk-blobs.txt`
- 日志：`verify-logs/zot-a.log`（101 周期全程 debug）、`gc-loop-result.log`、`gc-final-checks.log`、`scrub-baseline.log`、`scrub-corrupt{1,2,3a,3b-inplace,3b-consistent,3c}.log`、`scrub-after-gc.log`、`zot-b2.log`（在线 scrub 检出）、`zot-b3.log`（2h 钳制）、`zot-c.log`、`zot-nocompat.log`、`zot-mgmt.log`
