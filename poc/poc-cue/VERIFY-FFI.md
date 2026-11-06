# VERIFY-FFI.md — cuengine（Go-Rust FFI CUE 求值桥）补测

对应 `poc-cue/RESULT.md` ⑤遗留第 1 条："cuengine 未实测"。本次全部真实执行，2026-10-19，本机 16 核/14G。

## ① 结论

**可行（有明确构建前提）**。cuengine v0.40.6 能在 Rust 进程内求值 CUE，语义与 CLI 一致（约束拒绝、closed 拒绝、正例通过全部符合预期），性能 **p50 0.55 ms/次**（100 次实测），约为 CLI 进程路线（同目录实测 18 ms/次）的 **1/33**；且**运行时零网络**（`unshare -n` 断网下全用例通过）。已查证它确实包装 `cuelang.org/go`（crate 内 go.mod + bridge.go import 证据），不是无关项目。

代价：构建期需要 **Go 工具链（≥1.25）+ CGO**，build.rs 现场把 Go 桥编成静态库再链接进 Rust 二进制（debug 二进制 45 MB，含整个 Go 运行时 + CUE 库）；本环境 `proxy.golang.org` 双栈不可达，必须改用 `goproxy.cn`。API 形态是**按模块目录求值**（必须 `cue.mod/module.cue`），没有"求值内联字符串"的入口。

对 RESULT.md ④-6 路线建议的修正：运行时路线的代价从"进程路线 7-17 ms/次"降为"FFI 0.55 ms/次"，**高频（秒级/每次事件批）运行时求值从不可行变为可行**；"部署期预编译 JSON"仍是最省依赖的推荐（FFI 路线把 Go 工具链变成了构建期硬依赖）。

## ② 步骤（复现序列）

```bash
# 0. 前置：rustup/cargo 已有；Go 需另装
sudo apt-get install -y golang-go          # → go1.26.8（go.mod 要求 ≥1.25）

# 1. 草稿工程
cd poc/poc-cue/verify-ffi/
cargo new cuengine-probe && cd cuengine-probe
cargo add cuengine                          # v0.40.6
# 注意：若父目录是 cargo workspace，cargo new 会把自己挂进去，需从父 Cargo.toml members 移除

# 2. 构建依赖与网络（关键坑，见④）
export GOPROXY=https://goproxy.cn,direct    # proxy.golang.org 不可达
export GOTMPDIR=/home/pan-ding/gotmp        # /tmp 是 tmpfs，Go 编译中间产物爆配额
cargo build                                 # build.rs 现场编 Go 桥 → libcue_bridge.a

# 3. 求值探针（src/main.rs，见③）
cargo run
```

探针做了 5 件事：bridge 版本；最小求值（`price: int & >5` + 输入 42）；约束反例（输入 3）；closed schema 反例/正例（definition + 未知字段 owner）；100 次计时。另用 `sudo unshare -n cargo run 的产物` 复跑断网全用例，并用 CLI 对照计时。

## ③ 证据（命令原文与输出摘录）

**查证 crate 身份（不是无关项目）**——crate 源码 `go.mod`：
```
module github.com/cuenv/cuenv/libcue-bridge
require cuelang.org/go v0.16.0
```
`bridge.go` 头部 import：`cuelang.org/go/cue`、`.../cue/cuecontext`、`.../cue/load`，经 cgo `-buildmode=c-archive` 导出 `cue_eval_module` 等 C 符号。Cargo.toml.orig description："Go-Rust FFI bridge for CUE evaluation with production-ready features"。

**最小求值与正反例**（`cargo run` 输出原文）：
```
== 0. bridge 版本 ==
bridge version: bridge/1 (Go go1.26.8)

== 1. 最小求值 price: int & >5, 输入 42 ==
OK: {"price":42}

== 2. 反例: 输入 3 (违反 >5) ==
预期拒绝: CUE parsing failed at /tmp/cuengine-ffi-probe/mod: No instances could be evaluated
  (Hint: ... errors=[.: price: invalid value 3 (out of bound >5)], ...)

== 3. closed schema: definition 未知字段 ==
预期拒绝: ... errors=[.: rule.owner: field not allowed], ...)

== 4. closed schema 正例(无未知字段) ==
OK: {"rule":{"kind":"latency","params":{"threshold":120}}}
```
报错文本与 `cue vet` CLI（RESULT.md ③）逐字一致（`invalid value 3 (out of bound >5)` / `field not allowed`）。

**100 次求值计时**（同进程 FFI 调用，无进程 spawn）：
```
n=100  min=0.42ms  p50=0.55ms  mean=0.59ms  p95=1.02ms  max=1.34ms
```
CLI 对照（同目录 `cue export schema.cue input.cue` × 20）：**18 ms/次**。差约 33 倍。

**断网运行**：
```
$ sudo unshare -n ./target/debug/cuengine-probe
（全部 5 节用例原样通过，exit 0 —— 本地模块求值零网络）
```

**体积**：`libcue_bridge.a`（debug c-archive）49 MB；最终 debug 二进制 45 MB；Go 模块缓存 99 MB。

## ④ 对最终方案的反馈

1. **构建链是三件套**：Go ≥1.25 + CGO_ENABLED=1 + 模块代理。build.rs 在每次构建时现场执行 `go build -buildmode=c-archive bridge.go`，也就是说 **cargo 构建机的镜像里必须预装 Go 工具链**，且 Go 模块要么走可达代理要么 vendored。本环境实测：`proxy.golang.org` IPv6/IPv4 均 i/o timeout，`GOPROXY=https://goproxy.cn,direct` 后 200 OK（goproxy.io 也通）。
2. **tmpfs 坑**：/tmp 是 7.5G tmpfs，Go 编译 protobuf/x509 时 `disk quota exceeded`；`export GOTMPDIR=<大盘目录>` 解决。CI 里若 /tmp 小会遇到同样的错，报错不指明是配额问题。
3. **API 是目录级不是字符串级**：`evaluate_cue_package(dir, package)` 要求 dir 下有 `cue.mod/module.cue`，按 package 名求值目录内 .cue 文件；想"求值一段内联 CUE + JSON 输入"得自己落临时目录（本次探针即此做法），或用 `evaluate_module` 的 options（recursive/target_dir/with_meta/with_references）。**运行时输入注入要设计成"schema 文件预置 + 输入写临时文件再求值"**，0.55 ms 的代价可忽略，但临时文件生命周期要管。
4. **库内置了工程化细节**（lib.rs 实读）：每次 FFI 调用有 10s 硬超时（worker 线程 + channel），FFI 错误带结构化 code（INVALID_INPUT/LOAD_INSTANCE/…）和 Hint 字段（失败上下文里有逐实例错误原文，DX 好）；`get_bridge_version()` 可做健康检查。这些正是底座需要的，但注意超时后 worker 线程是 detached 的（库文档自认无法安全中止 Go 侧调用）。
5. **定位建议**：FFI 路线适合"底座进程内、秒级频率"的运行时求值（如配方加载/合并、每事件批校验）；预编译 JSON 仍是首选主路线（零 Go 依赖、零运行时开销）。两者不互斥：同一套 cue.mod 工件，部署期 `cue export` 预编译，异常路径回退 FFI 现算。

## ⑤ 遗留问题

- 未测 `evaluate_module` 的 `with_meta/with_references`（源位置/引用路径提取）——cuenv 自己的核心用例，底座若要"错误定位到行"需要它，本次只验证了求值主路径。
- 未测 release 构建（本次 debug；release LTO 后二进制会小一些，但 Go 运行时的量级不会变）。
- 未测含外部依赖（`import "github.com/..."`）的模块——那会触发 bridge 里 Registry 的 OCI/CUE registry 拉取，网络语义未验证；底座 schema 若自包含则不涉及。
- 千条规则大配方下的 FFI 求值性能未测（CLI 侧也未测，同待补）。
- 构建产物（libcue_bridge.a 49 MB）是否可预编译分发（`nix-build` feature 名暗示上游有此路径）未验证；若可行可把 Go 依赖从 cargo 构建机上摘掉。

## 附：本次产物

- `poc/poc-cue/verify-ffi/cuengine-probe/`——草稿工程（Cargo.toml / src/main.rs 即探针全文；target/ 已清理）
