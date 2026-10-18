# BOOTSTRAP.md — PoC 开发环境引导记录

生成时间：2026-10-19（由环境引导代理在本机实际执行并验证）

## 仓库布局

```
af/                          ← 本目录（仓库根）
├── Cargo.toml               ← cargo workspace 根（members=["crates/*"]，edition="2021"）
├── bin/                     ← 单二进制工具
├── poc/
│   ├── poc-dspy/.venv/      ← Python venv（dspy）
│   └── poc-temporal/.venv/  ← Python venv（temporalio）
└── crates/                  ← 11 个空壳 crate
```

## 版本号（本机实测）

| 工具 | 版本 | 验证命令与输出摘要 |
|---|---|---|
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) | `source ~/.cargo/env && rustc --version` |
| cargo | 1.99.1 (5f94df478 2026-08-27) | `cargo --version` |
| rustup | 1.29.1 (d95a37b6a 2026-08-13) | `rustup --version`，toolchain=stable，profile=minimal |
| temporal | 1.9.1 (Server 1.32.0, UI 2.54.1) | `bin/temporal --version` |
| zot | v2.1.22 (commit v2.1.22-0-g6bbc7e5, dist-spec 1.1.1) | `bin/zot --version`（输出 JSON 日志行，见下"注意事项"） |
| cosign | v3.1.3 (GoVersion go1.26.4) | `bin/cosign version` |
| cue | v0.17.1 (CUE language v0.17.1) | `bin/cue version` |
| crane | 0.22.1 | `bin/crane version` |
| python | 3.14.4 | `python3 --version` |
| temporalio (pip) | 1.34.0 | `import temporalio` 成功 |
| dspy (pip) | 3.4.1 | `import dspy` 成功 |

## binaries 状态（af/bin/，均已 chmod +x 并跑通 --version）

| 二进制 | 状态 | 来源 |
|---|---|---|
| temporal | ✅ 可执行 | `https://temporal.download/cli/archive/latest?platform=linux&arch=amd64`（tar.gz 解出 `temporal`） |
| zot | ✅ 可执行 | GitHub `project-zot/zot` release v2.1.22 资产 `zot-linux-amd64`（经 API releases/latest 查得） |
| cosign | ✅ 可执行 | GitHub `sigstore/cosign` release v3.1.3 资产 `cosign-linux-amd64` |
| cue | ✅ 可执行 | GitHub `cue-lang/cue` release v0.17.1 资产 `cue_v0.17.1_linux_amd64.tar.gz`（解出 `cue`） |
| crane | ✅ 可执行（来源有偏差，见下） | GitHub `google/go-containerregistry` release v0.22.1 资产 `go-containerregistry_Linux_x86_64.tar.gz`（解出 `crane`） |

**crane 下载偏差说明**：任务指定的资产名 `crane-linux-amd64` 在最新 release（v0.22.1）中已不存在——用该 URL 下载得到 9 字节 "Not Found"（HTTP 404，curl 中途 SSL eof exit 56）。经 `api.github.com/repos/google/go-containerregistry/releases/latest` 确认现资产为 `go-containerregistry_Linux_x86_64.tar.gz`（内含 crane/gcrane/krane），已下载并解出 crane 重命名就位，功能等价。

## cargo workspace 状态

- 根 `Cargo.toml`：`[workspace]` `members=["crates/*"]` + `resolver="2"`；`[workspace.package]` `edition="2021"`；`[profile.dev]` 未动。
- 11 个 crate（`cargo new --lib` 创建，均继承 `edition.workspace = true`）：
  af-credential、af-graph、af-workorder、af-effect-gateway、af-sampling、af-release-gate、af-meter、af-calendar、af-trace、af-stop、af-station
- 空壳构建验证：`cargo build -j 4` → `Finished \`dev\` profile` ✅（11 crate 全部编译通过）

## venv 状态

| venv | 状态 | 内容 |
|---|---|---|
| `poc/poc-dspy/.venv` | ✅ 建成 | dspy 3.4.1 装入成功（Python 3.14.4 上 `import dspy` 通过；pip 走清华镜像，cp314 wheel 齐全，未遇到兼容性失败） |
| `poc/poc-temporal/.venv` | ✅ 建成 | temporalio 1.34.0 装入成功，`import temporalio` 通过 |

激活方式：`source poc/poc-dspy/.venv/bin/activate`（temporal 同理）。

## 给后续代理的注意事项

1. **cargo 不在默认 PATH**：每条 shell 命令前需 `source ~/.cargo/env`，或直接用 `~/.cargo/bin/cargo`。
2. **构建加 `-j 4`**（`cargo build -j 4` / `cargo test -j 4`），避免并行编译压垮机器。
3. **crate 新增依赖后**记得在 workspace 根统一管理版本（`[workspace.dependencies]`），成员里用 `foo.workspace = true`。
4. **temporal.download 的参数**：正确写法是 `?platform=linux&arch=amd64`（任务原文的 `platform=linux-amd64` 会被服务端拒绝，返回 80 字节纯文本错误而非归档）。该端点对 HEAD 返回 405，只能 GET。
5. **GitHub release 资产下载单连接很慢**（实测 zot 215.9MB 跑了约 35 分钟，峰值 ~60-250KB/s）；大文件建议后台跑或并行多任务。偶发 `SSL unexpected eof (curl exit 56)` / 连接超时，重试即可（建议 `--retry 3 --retry-all-errors`）。
6. **`zot --version` 的输出不是常规版本字符串**，而是带时间戳的 JSON 日志行（message=version，含 commit 与 distribution-spec）；脚本判断版本时按 JSON 解析。
7. **pip 走清华镜像**（pypi.tuna.tsinghua.edu.cn，机器预置）；Python 3.14 下 dspy/temporalio 均有 cp314 轮子，直接装即可。
8. bin/ 下二进制按名字直接调用，如 `./bin/cosign version`；bin 目录已加入仓库根，注意别把大二进制误提交（如需 git，请在 .gitignore 处理 `bin/` 与 `target/`）。
