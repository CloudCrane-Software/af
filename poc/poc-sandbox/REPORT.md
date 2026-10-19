# poc-sandbox 报告：CubeSandbox 实测

任务 id：poc-sandbox ｜ 日期：2026-10-19 ｜ 执行环境：本机（Ubuntu，内核 7.0.0-38-generic，16C/14GiB，/dev/kvm 可用）

---

## ① 结论

**CubeSandbox 实测通过，且全链路在本机真实跑通**（未降级到文档级评估）：

1. **可行性**：`/dev/kvm` 存在且当前用户可写 → 走 bare-metal 路径（原生 KVM），无需 PVM 换内核。release bundle v0.7.3 一键安装成功，16 个 `cube-sandbox-*` 服务 + 2 个 target 全部 running/exited，E2B 兼容 API `/health`=200。
2. **最小闭环全通**：创建沙箱（143ms）→ 执行 shell（guest 内核 `6.6.1199-0009`，与宿主 `7.0.0-38` 不同 → 硬件级 microVM 隔离实证）→ 文件写入/取出（内容逐字节一致，沙箱内 `cat` 复核）→ 快照（570ms）→ 回滚恢复（319ms，v2→v1 状态精确恢复）→ 从快照克隆新沙箱（522ms，状态延续）。
3. **冷启动（10 次，从 READY 模板）**：create p50=156.5ms（min 114.6 / mean 163.2 / p90 200.0 / max 206.3ms，σ=31.0）；首条命令 p50=52.8ms。与 README 宣称"亚 60ms"有差距——本机为 WiFi 主机 + loop-XFS 存储 + 控制面同机部署，且测量含 SDK→API RTT；量级（百 ms 级）成立。
4. **内存**：每沙箱 = 1 个 `containerd-shim-cube-rs` 进程（**内嵌 VMM**，运行期直接持有 `/dev/kvm`，无独立 hypervisor 子进程），实测 PSS 176.6MB（guest 配置 2GiB/2vCPU，惰性分配故远小于上限）；宿主常驻：cubelet 293MB + 控制面其余约 91MB + 支撑容器（MySQL/Redis/MinIO/CoreDNS/proxy 等）约 268MiB。
5. **Rust 组件可自建**：`hypervisor`（cloud-hypervisor 分支）、`cubecow`（XFS reflink CoW 引擎）、`CubeAPI` 三个 Rust crate 均 `cargo build --release -j4` 成功（缺 `libseccomp-dev`/`libcap-ng-dev` 与 workspace 隔离问题已解决，见②）。
6. **License**：Apache-2.0（Tencent LICENSE 文件，含第三方例外清单）；repo 主语言 Go（10.2MB）+ Rust（8.1MB）+ C，12974 stars，活跃维护（v0.7.3 构建于 2026-10-12）。

**关键决策输入**：CubeSandbox 可作为自托管沙箱底座直接进入集成评估；其 E2B 兼容 API + 原生 SDK 的快照/克隆/回滚原语（非 E2B 上没有的能力）是相对纯 E2B 云服务的主要增量。工具 broker 的接口形态建议直接对标其 SDK 面（见④）。

---

## ② 步骤（全部真实执行）

1. **KVM 检查**：`ls -la /dev/kvm` → 存在；`getfacl` 显示 `user:pan-ding:rw-`；`[ -w /dev/kvm ]` 通过；`kvm_intel` 已加载；`/proc/cpuinfo` 32 行含 `vmx`。
2. **定位 repo**：GitHub API 搜索 `cubesandbox` → `TencentCloud/CubeSandbox`（Go 10.2MB/Rust 8.1MB/C 6.9MB，license NOASSERTION→克隆后读 LICENSE 确认 Apache-2.0）。
3. **获取源码**：`git clone https://github.com/...` 失败（`github.com:443` 连接被断，curl 探测=000）；改用 `codeload.github.com` tarball（33.9MB，master@2026-10-18）解压到任务目录 `CubeSandbox/`。
4. **部署路径选型**：读 `docs/guide/quickstart.md`——本机已有 KVM → 不走 PVM（需换内核重启），走 `bare-metal-deploy.md`。前置逐项核验：glibc 2.43≥2.31 ✅；根分区 ext4 → `apt install xfsprogs` + 60G 稀疏 loop 镜像 `mkfs.xfs` 挂载 `/data/cubelet` ✅；Docker 29.9 ✅；`tencentcloudcr.com` 可达（401=正常鉴权挑战）✅；MySQL/Redis/MinIO/CoreDNS 镜像全部来自 `cube-sandbox-image.tencentcloudcr.com`（**不依赖 docker.io**，与本机 docker.io 不可达不冲突）。
5. **安装**：`online-install.sh` 会把 bundle 下到 `/tmp`（tmpfs 配额小）→ 自行从 CN 镜像 `download.cubesandbox.com/release/latest.json` 指到的 cnb.cool 下载 v0.7.3 bundle（281MB）到任务目录，直接跑 bundle 内 `install.sh`。
6. **端口/网段避让**（本机已有其他项目常驻，不能动）：gatehouse compose 栈占 `3000/6379/5432`、mihomo 占 `9090`、LAN `192.168.1.0/24` 与默认沙箱网段 `192.168.0.0/18` 冲突 → 设 `CUBE_API_BIND=0.0.0.0:13000`、`CUBE_SANDBOX_REDIS_PORT=6380`、`CUBE_PROXY_GRPC_PORT=9190`、`CUBE_SANDBOX_NETWORK_CIDR=10.66.0.0/16`。
7. **踩坑与修正**（重要）：环境变量传给 `sudo install.sh` **不会传导到 systemd 服务**——单元用 `EnvironmentFile=/usr/local/services/cubetoolbox/.one-click.env`（`scripts/systemd/common.sh:106` source），首次安装后 compose 仍渲染 6379。修正：把端口覆盖追加进 `.one-click.env`，重跑 `install.sh`（upgrade 模式，`ONE_CLICK_ASSUME_YES=1`，three-way merge 保旧值）→ compose 渲染 `127.0.0.1:6380:6379` ✅。
8. **模板创建**：registry 延迟实测 cn=152ms / int=14.7s → 用 cn；`cubemastercli tpl create-from-image --image cube-sandbox-cn.tencentcloudcr.com/cube-sandbox/sandbox-code:latest --writable-layer-size 1G --expose-port 49999 --expose-port 49983 --probe 49999` → 提交到 READY 约 30s，得 `tpl-88d8d5fc55794037b14ecbe0`。
9. **DNS 踩坑与绕过**：SDK 数据面需解析 `<port>-<sandbox_id>.cube.app`；本机 `/etc/resolv.conf` 是既有 smartnet dnsmasq（127.0.0.1，非 systemd-resolved stub），安装器写入 resolved 的 `~cube.app` 路由不生效（直查 CoreDNS 169.254.254.53 是 NOERROR）。**不改系统 DNS**，改用 cubesandbox SDK 内置的解析旁路：`CUBE_PROXY_NODE_IP=192.168.1.120`（`_transport.py` 实现 curl --resolve 等价物，保 Host 头直连 CubeProxy）→ 闭环全通。
10. **SDK**：任务 venv 安装 `cubesandbox==0.7.3`、`e2b-code-interpreter==2.11.0`（清华镜像）。闭环与压测用 `cubesandbox` 原生 SDK（`closed_loop.py`、`bench_coldstart.py`、`probe_memory.py` 留存任务目录）。
11. **cargo 构建**（-j4，release）：
    - `hypervisor`：首次失败——`rust-lld: unable to find library -lseccomp/-lcap-ng`；`apt install libseccomp-dev libcap-ng-dev` 后成功（增量 50.4s，产物 `cube-hypervisor` 5.7MB + `ch-remote`）。
    - `cubecow`：首次失败——cargo 向上探测进 AF workspace（`current package believes it's in a workspace when it's not: workspace: af/Cargo.toml`；cubecow 自己的 Cargo.toml 只在注释里提过 `[workspace]`）。在克隆树内给 `cubecow/Cargo.toml` 追加空 `[workspace]` 表后成功（60.1s，产物 `cubecow-cli`）。同样给无 workspace 表的 `guest-init/Cargo.toml`、`CubeAPI/Cargo.toml` 追加（仅改任务目录内克隆树，**未动 AF 根 Cargo.toml**）。cubecow 通过 `rust-toolchain.toml` 钉住 1.89（rustup 自动装）。
    - `CubeAPI`：3m36s wall（248 crates）成功，产物 `cube-api` 9.3MB。
12. **测量**：冷启动 10 次（脚本见③）；单沙箱运行期采样 `/proc/*/smaps_rollup` PSS 与 `fuser /dev/kvm`；容器层 `docker stats --no-stream`。
13. **清理**：见⑤（沙箱进程已全部退出；systemd 服务停用见⑤遗留清单）。

---

## ③ 证据（命令原文 + 输出摘录）

**KVM**：
```
$ ls -la /dev/kvm
crw-rw----+ 1 root kvm 10, 232 Oct 15 11:33 /dev/kvm
$ getfacl /dev/kvm   →  user:pan-ding:rw-
$ grep -cE 'vmx|svm' /proc/cpuinfo → 32 ; lsmod | grep kvm → kvm_intel 552960
```

**GitHub 直连与替代路径**：
```
$ curl -s -o /dev/null -w "%{http_code}" https://github.com/        → 000（连接被断）
$ curl ... https://codeload.github.com/TencentCloud/CubeSandbox/tar.gz/refs/heads/master → 200, 33,901,168 bytes
$ curl https://download.cubesandbox.com/release/latest.json
→ {"url": "https://cnb.cool/.../cube-sandbox-one-click-v0.7.3-amd64.tar.gz"}（content-length 294,846,426）
```

**License / 语言**（GitHub API + 克隆树）：
```
languages: {"Go":10218848, "Rust":8089301, "C":6905815, ...}  stars: 12974
$ head LICENSE → "Cube Sandbox is licensed under the Apache-2.0 except for the third-party components listed below."
```

**安装后服务与端口**：
```
$ systemctl list-units 'cube-sandbox-*' → 16 个服务 running/exited（coredns、cube-api、cubelet、cubemaster、
  cubeops、cube-proxy、templatecenter、mysql、redis、minio、webui、egress、lifecycle-manager、dns…）
$ curl http://127.0.0.1:13000/health → 200
$ sudo grep 638 /usr/local/services/cubetoolbox/support/docker-compose.yaml
38:      - "127.0.0.1:6380:6379"
```

**模板创建**（cn registry，延迟 cn=152ms vs int=14,705ms）：
```
$ cubemastercli tpl create-from-image --image cube-sandbox-cn.tencentcloudcr.com/cube-sandbox/sandbox-code:latest ...
11:23:33 submitted ... job_id=fed6f35b... template_id=tpl-88d8d5fc55794037b14ecbe0
11:23:33 [1/7] PULLING … 11:24:02 [7/7] READY progress=100%
$ cubemastercli tpl list → tpl-88d8d5fc55794037b14ecbe0 READY
```

**最小闭环**（`closed_loop.py`，env：CUBE_API_URL=http://127.0.0.1:13000、CUBE_PROXY_NODE_IP=192.168.1.120）：
```
[OK] sandbox.create  (143 ms)  -> id=564c9518e2904f8eb9d41259d7287955
[OK] commands.run('echo...')  (161 ms)  -> hello-from-cube / 6.6.1199-0009-03_2.0.1 / 0
[OK] files.write (6 ms) / files.read (4 ms) -> cube-file-exchange-payload-2026
[OK] cat inside sandbox (15 ms) -> cube-file-exchange-payload-2026   ← 沙箱内复核一致
[OK] create_snapshot (570 ms) -> snap-3a2b92fd76e0418097fe179f
[OK] rollback (319 ms) -> status READY；断言 v2→rollback→v1 通过
[OK] clone from snapshot (522 ms) -> v1（克隆沙箱状态延续断言通过）
CLOSED_LOOP_ALL_OK
```
（guest 内核 `6.6.1199-0009-03_2.0.1` ≠ 宿主 `7.0.0-38-generic` → microVM 隔离实证）

**冷启动 10 次**（`bench_coldstart.py`）：
```
run01..10 create=174/164/138/115/141/145/206/200/199/149 ms
CREATE_MS     {'n': 10, 'min': 114.6, 'p50': 156.5, 'mean': 163.2, 'p90': 200.0, 'max': 206.3, 'stdev': 31.0}
FIRST_CMD_MS  {'n': 10, 'min': 33.5,  'p50': 52.8,  'mean': 54.1,  'p90': 65.1,  'max': 86.7,  'stdev': 15.7}
```

**内存**（单沙箱运行期采样）：
```
$ sudo ps -eo pid,rss,args | grep -i cube（摘录）
  cubelet                 RSS=278~293MB   （节点常驻，随 GC 波动）
  containerd-shim-cube-rs RSS=179,548kB  PSS=176,620kB   ← -id 33b4859ad8…（每沙箱 1 个）
$ sudo fuser -v /dev/kvm（沙箱运行期）
/dev/kvm:  root  633407 F.... containerd-shim          ← shim 内嵌 VMM 直接持有 KVM
guest（SDK run_code）: /proc/meminfo MemTotal: 1,976,536 kB（≈2GiB 配置，惰性分配）；os.cpu_count()=2
$ docker stats --no-stream（cube 相关容器）：mysql 78.75MiB、cube-proxy 71.52MiB、minio 64.5MiB、
  coredns 10.2MiB、redis 7.7MiB、egress 13.05MiB、lifecycle-manager 9.34MiB、webui 12.9MiB ≈ 268MiB 合计
控制面进程 RSS：cubelet 293MB、cubemaster 32.6MB、templatecenter 26.3MB、cubeops 18.9MB、cube-api 13.6MB
沙箱结束后：pgrep containerd-shim-cube-rs → 0（进程模型随沙箱生灭，无残留）
```

**cargo 构建**：
```
hypervisor: 首次 error: rust-lld: unable to find library -lseccomp / -lcap-ng → exit 101
            apt 装 libseccomp-dev 2.6.0 / libcap-ng-dev 0.8.5 后 → exit 0，Finished release in 50.43s（增量）
cubecow:    首次 error: current package believes it's in a workspace when it's not（探测到 af/Cargo.toml）→ exit 101
            克隆树内追加空 [workspace] 后 → exit 0，real 1m00.1s（产物 cubecow-cli）
CubeAPI:    exit 0，real 3m35.9s，248 crates（产物 cube-api 9.3MB）
```

**SDK 面**（`inspect(Sandbox)`，实测对象方法）：
```
create/connect/kill/pause/resume/rollback/clone/create_snapshot/delete_snapshot/list_snapshots/
list/list_v2/set_timeout/get_info/health/update_network/run_code/commands/files/pty/get_host/…
Sandbox.create(template=None, *, timeout=None, env_vars=None, metadata=None,
               allow_internet_access=True, network=None, lifecycle=None,
               volume_mounts=None, mcp=None, ...)
```

---

## ④ 对方案的反馈 / 决策输入

### 4.1 选型结论
- **自托管路径可行且成本低**：bundle 安装 + 一条模板创建命令即可用，无需换内核（有 KVM 的机器）、不依赖 docker.io（镜像走腾讯 registry）。Rust 核心组件（VMM/shim/存储引擎）可自建、可改造，符合"语言策略偏好 Rust 组件"。
- **与 E2B 云的关系**：CubeAPI 兼容 E2B SDK 协议（控制面 REST `POST /sandboxes`=201 实测），存量 E2B 代码可平移；但**快照/回滚/克隆是 Cube 原生 SDK 独有原语**，是 broker 应该利用的增量能力。注：`e2b-code-interpreter` SDK 的数据面在本机未端到端跑通（需官方 dev-sidecar 或泛解析 DNS），本次闭环全部走 `cubesandbox` 原生 SDK——选型时按"原生 SDK 为主、E2B 兼容为辅"规划。

### 4.2 工具 broker IDL 需求清单（从实测反推）

实测暴露的接口形态与坑，逐条映射为 broker 对沙箱后端的 IDL 需求：

**A. 生命周期 / 进程模型**
- `Create(template_id, timeout?, env_vars?, metadata?, network?, volume_mounts?, allow_internet_access?) -> SandboxRef{id, domain, expires_at}`——实测模板冷启动 p50≈157ms，broker 可按"每任务一沙箱"建模而不必池化；timeout 是**一等参数**（SDK `set_timeout` 可续期），IDL 必须有显式 TTL + 续约。
- `Kill(id)`：实测进程随沙箱生灭（shim 退出、`pgrep`=0）；broker 需要 idempotent kill + 对孤儿沙箱的后台对账（list 语义）。
- 进程模型：**每沙箱 = 1 个宿主进程（shim 内嵌 VMM，持 /dev/kvm）**。broker 侧资源统计应以该进程 RSS/PSS 为准（实测 176MB/沙箱 + guest 上限 2GiB 惰性）；SDK `get_info`/`health` 可作心跳。

**B. 执行**
- `RunCommand(id, cmd, cwd?, env?, timeout?) -> {exit_code, stdout, stderr}`（`commands.run`，实测 p50≈53ms/条）。
- `RunCode(id, code, language?) -> {logs, results, error}`（code-interpreter 通道，独立于 shell，实测 292ms）。
- 两者时延差距 5 倍+，IDL 应分开建模，不要用同一接口硬套。

**C. 文件交换**
- `WriteFile(id, path, bytes)` / `ReadFile(id, path) -> bytes`：实测往返 6ms/4ms 且**写入在沙箱内 shell 可见**（同一根文件系统）；IDL 需要明确的可见性契约（写入即持久、跨执行通道一致）。
- 大文件路径未测（≥MB 级走何通道、是否分块），列入⑤遗留。

**D. 快照 / 恢复（Cube 特有，broker 应暴露为原语而非内部细节）**
- `Snapshot(id) -> SnapshotId`（实测 570ms）、`Rollback(id, SnapshotId)`（319ms，进程内恢复，SDK 连接自动重置）、`Clone(SnapshotId|SandboxId, n?) -> SandboxRef[]`（实测 522ms 单个，官方支持并发扇出与跨节点 pause/resume[需 S3]）。
- 语义要点（实测验证）：快照独立于沙箱生命周期（可先 kill 后克隆）；回滚恢复**文件系统+内存**；克隆体继承状态但彼此隔离。
- broker 用途映射：任务级 checkpoint/重试 = snapshot→rollback；并行发散 = clone(n)；长闲置任务省内存 = pause/resume。

**E. 资源上限**
- 模板级：`--writable-layer-size`（磁盘）、CPU/内存规格（本模板 2vCPU/2GiB，guest `MemTotal` 实证）；安装期 `/data/cubelet` 要求 XFS（reflink CoW）≥50GB。
- 运行级：沙箱网段是**整段 CIDR 预留**（本机 10.66.0.0/16，需与宿主路由/内网不冲突，安装器强校验）；`cgroup v2 cpu` 控制器、bpffs 为硬前置。
- IDL 需求：资源配额（cpu/mem/disk/net）在**模板创建时声明**而非运行时改（实测接口面如此）；broker 需在池/节点层面管理 CIDR 与磁盘余量。

**F. 网络与寻址（实测最大的集成坑）**
- 数据面寻址 = `<port>-<sandboxId>.<domain>`（默认 cube.app，443/80 经 CubeProxy），**依赖泛解析 DNS**；无泛解析环境要么起官方 dev-sidecar，要么用 SDK 的 `CUBE_PROXY_NODE_IP` 直连旁路（保 Host 头）。
- IDL 需求：broker 不应把 DNS 假设泄漏进接口——后端适配层要能注入"直连地址+Host 重写"；同时每沙箱暴露端口需在**创建时声明**（模板 `--expose-port`，运行期 `update_network` 可调），未声明端口不可达。

**G. 认证 / 多租户**
- 默认无鉴权（`CUBE_API_KEY` 未设即关闭，官方仅建议内网用）；`E2B_API_KEY` 任意占位串可过 SDK。broker 若多租户，必须自带 API-key 签发与每请求头注入（官方有 authentication.md 路径，本次未启用）。

**H. 部署形态输入**
- 单机 all-in-one 最小可用；控制面（Go）+ 节点代理（cubelet, Go）+ 每沙箱 shim（Rust）+ CoW 存储引擎（cubecow, Rust）；依赖 MySQL+Redis+MinIO(+CoreDNS/CubeProxy)。
- **配置热路径**：`/usr/local/services/cubetoolbox/.one-click.env` 是 systemd 服务的唯一配置入口——broker 的部署器必须写这里而不是进程环境（本次 6379→6380 踩坑实证）。

---

## ⑤ 遗留

1. **系统状态**（本机残留，均可逆）：
   - 已停止全部 `cube-sandbox-*` 服务并确认端口释放（13000/6380/9190/443/3306 均空）。重启用：`sudo systemctl start cube-sandbox-control.target cube-sandbox-compute.target`。
   - XFS loop 挂载保留：`/dev/loop24 → /home/pan-ding/cube-data/cubelet-xfs.img(60G 稀疏) → /data/cubelet`（模板数据在其中）。彻底清理：停服务后 `sudo umount /data/cubelet && rm /home/pan-ding/cube-data/cubelet-xfs.img`。
   - 克隆树内改动：`cubecow/Cargo.toml`、`CubeAPI/Cargo.toml`、`guest-init/Cargo.toml` 各追加空 `[workspace]`（防吸进 AF workspace；**AF 根 Cargo.toml 未动**）。
2. **未测项**：
   - 并发压测（clone n>1、多沙箱并行冷启动）、大文件交换、`pause/resume` 跨节点（需 S3 后端）、网络策略（allowlist/断网 `unshare -n`）未执行。
   - `e2b-code-interpreter` SDK 端到端（数据面需 dev-sidecar）未跑通，E2B 兼容性结论基于控制面 REST + 原生 SDK。
   - youki/containerd 级 Rust 沙箱对照表未做（未触发降级条件，故按任务主路径完成）。
3. **环境注意**：`/tmp`（tmpfs 7.5G）配额紧，安装类操作避开；`github.com:443` 直连被断（api/codeload/cnb.cool 可用）；本机 `/etc/resolv.conf` 为既有 smartnet dnsmasq，任何依赖 systemd-resolved 泛解析的方案都需绕行。
4. **量测口径**：冷启动含 SDK→控制面 RTT 与 Python 开销，非纯内核启动时间；官方宣称"亚 60ms/<5MB"口径未知，本报告数字为端到端实测。
5. **清理证据**：
```
$ sudo systemctl stop cube-sandbox-control.target cube-sandbox-compute.target
$ sudo systemctl disable cube-sandbox-control.target → Removed '.../multi-user.target.wants/cube-sandbox-control.target'
$ systemctl list-units 'cube-sandbox-*' | grep -c running → 0
$ 端口复核：13000/6380/9190/443/3306/9000 全部 released
$ cube 的 containerd-shim-cube-rs 进程 → 0（残留的 3 个 containerd-shim 为 docker 原生
  containerd-shim-runc-v2 -namespace moby，属先于本任务存在的 gatehouse 容器，未触碰）
```

---
证据文件（任务目录 `af/poc/poc-sandbox/`）：`closed_loop.py`、`bench_coldstart.py`、`probe_memory.py`、`probe_memory.out`、`closed_loop` 输出（上文转录）、`install.log`、`tpl-create.log`、`build-{hypervisor,cubecow,cubeapi}.log`、`CubeSandbox/`（master@2026-10-18）、`downloads/cube-sandbox-one-click-v0.7.3-amd64/`。
