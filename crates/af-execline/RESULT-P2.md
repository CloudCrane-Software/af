# RESULT-P2 —— af-bench 吸收 · 任务包 P2（宿主桥接抽象）

> 身份：吸收施工员 P2（P2-F04），2026-10-24。工作树 `af-wt-phase2-p2`（分支
> `phase2-execline-p2`，基于 P1 完成点 6ee05f8），权威施工图
> `计划/研究组/af-bench吸收方案-20261024.md` §5-P2（§2.3 trait 形态）。
> 施工形态：ADR-007 先红后绿——RED `dd3bc28`（验收测试 + trait/生命周期
> `unimplemented!()` 桩，p2_bridge 10 红/2 过/1 ignore）→ GREEN `ee0a6e0`
> （原样搬入转绿）→ 重构 `584e462`（clippy useless_concat，语义不变）。
> af-bench / af-workorder / af-engine 零改动
> （`git diff 6ee05f8..HEAD --stat -- crates/af-bench crates/af-workorder
> crates/af-engine` 为空，实测）；未写 计划/变更日志.md；未碰 P3-P6 内容
> （capture_diff/verify/judge 属 P3，AgentRunner/SwevExecuteUnit 属 P4，
> TbEnv::verify/TbExecuteUnit/driver 接线属 P5，薄壳化属 P6）。

## 1. 做了什么（源 af-bench 文件:行号 → 落位）

af-bench 源 = 本工作树 `crates/af-bench/src/`（开工 `diff` 实测与主仓
tb.rs/swev.rs 逐字节一致）。

| 搬入物 | af-bench 源 | af-execline 落位 | 迁移形态 |
|---|---|---|---|
| `HostBridge` trait | 方案 §2.3 设计（`fn run(&self, host, script, timeout_sec) -> Result<(bool,String),String>`） | `src/hostbridge.rs:20` | 新增缝；对象安全（`&dyn` 可持，有测试固化） |
| `SshBridge`（真实现） | tb.rs:38-64 `ssh` + swev.rs:268-293 `tb_ssh` 收敛 | `src/hostbridge.rs:33,35` | **方案 §1.1 唯一合并点**：两函数语义同构，行为以 tb.rs:42-64 为准；函数体逐语句原样（`ssh` 并入 trait 更名 `run`，`shell_quote` 改调 P1 已迁副本）。两处 format 模板产出**逐字节相等**已由推导程序实测证实（见 §2 黄金值来源） |
| `TbEnv`（struct + 字段） | tb.rs:94-99 | `src/suite/tb.rs:134` | 原样（字段与 pub 可见性保留，P5 verify 消费） |
| `TbEnv::setup` | tb.rs:122-171 | `src/suite/tb.rs:144` | 原样（`ssh()` 换 `bridge.run()`；docker build 代理 build-args / 前置 `docker rm -f` / run 护栏 2g·256 + 容器级代理 + PAGER·TQDM 卫生组 / 超时表 1800·30·60 / 错误文案 tail 800·400 逐字节不变） |
| `TbEnv::workdir` | tb.rs:173-176 | `src/suite/tb.rs:200` | 原样 |
| `TbEnv::exec` | tb.rs:178-222 | `src/suite/tb.rs:213` | 原样（`docker exec … ; echo __AF_RC=$?` 构造、`split_rc` 剥标记行、超时教学文案、桥接失败可重试文案、CAP 12_000 截断、`exit=` 末行全部逐字节保留） |
| `TbEnv::cleanup` | tb.rs:294-296 | `src/suite/tb.rs:252` | 原样（`docker rm -f`，超时 60） |
| `swev::setup` | swev.rs:54-99 | `src/suite/swev.rs:128` | 原样（`tb_ssh()` 换 `bridge.run()`；容器名 `afbench-swev-<task 中 `__`→`-`>-<profile>`、前置 `docker rm -f` 30s、代理三分支 local 空/anolis 三变量、护栏 6g·512、卫生组、`sleep infinity`、超时 120、`起容器失败:` 头截 400 与桥接 Err 原样透传逐字节不变） |

**未搬（按方案范围纪律）**：`TbEnv::verify`（tb.rs:226-292，P5 TB 单元）、
`swev::capture_diff`/`verify`/`sha256_hex`（swev.rs:102-266，P3）、`AgentRunner`
与 `SwevExecuteUnit`（P4）、`ExecContext`/`ExecOutput` 扩展（P4）、RAII cleanup
guard（P4，方案 §2.4）。判分三件套契约语义未动（本包不触及判分链）。

## 2. 测试清单（tests/p2_bridge.rs，13 例：12 跑全绿 + 1 ignore）

方案 §5-P2 验收测试草案逐项落位：

| 测试 | 覆盖（草案条目） |
|---|---|
| `swev_setup_anolis_builds_guardrail_proxy_hygiene_command` | swev setup 命令黄金串全等：`--memory 6g --pids-limit 512`、anolis 三代理变量、PAGER/TQDM 卫生组、前置 `docker rm -f`、超时表 30/120、容器名 `__`→`-` 折叠、workdir=/testbed |
| `swev_setup_local_omits_proxy_env` | local 分支无任何 PROXY 变量、护栏与卫生组仍在；`--pids-limit 512` 后**双空格**原样疵点保留（黄金串全等） |
| `swev_setup_run_failure_error_messages` | `起容器失败:` chars().take(400) 头截断；桥接 Err 原样透传（swev.rs:93-97） |
| `tb_setup_and_cleanup_command_sequence` | tb setup 三连命令黄金串全等（build 代理 build-args+1800s / 前置 rm 30s / run 2g·256+卫生组+60s）、cleanup `docker rm -f` 60s、workdir=/app |
| `tb_setup_failure_error_messages` | `build {image} 失败:` tail 800 尾截断、`起容器 {container} 失败:` tail 400 |
| `tb_exec_normal_splits_marker_and_appends_exit_line` | exec 命令构造（cd 前缀 + `; echo __AF_RC=$?`，超时 180）；剥标记行保留前后换行；`exit=0` 末行 |
| `tb_exec_timeout_without_marker_teaches_and_exits_124` | 外层超时无 `__AF_RC` → 教学文案逐字节 + `exit=124`（tb.rs:201-205） |
| `tb_exec_bridge_failure_reports_transient_infrastructure` | 桥接启动失败 → "基建瞬断、可重试"文案逐字节 + `exit=127`（tb.rs:194-199） |
| `tb_exec_truncates_at_cap_12000` | 截断 CAP 12_000：头 6_000 + `8002 bytes elided` 标记 + 尾 6_000 + `exit=0`（tb.rs:206-215） |
| `ssh_bridge_local_branch_runs_bash_merges_stderr_and_status` | 真实现 local 冒烟（真 bash+timeout 子进程，零 docker/ssh/网络，P1 resolve_key printf 同类）：stdout/stderr 合并语义、非零退出、单引号脚本经 `shell_quote` 穿透两层 `bash -c` |
| `smoke_real_local_docker_via_bridge`（`#[ignore]`） | 草案"真 local docker 冒烟 1 例（有 docker 才跑）"：经桥跑 `docker version`，busybox 镜像在本机时走最小容器生命周期（rm→run 带护栏→exec→rm）；无 docker/无镜像打印说明跳过，不假绿。**本任务未执行**（资源纪律禁碰 docker） |
| `cmp_p2_templates_against_bench_source` | 源文本比对：桥接包裹模板（tb.rs:43 与 swev.rs:269-271 同构=收敛依据）、swev/tb 全部命令模板、截断与两段失败文案原文在 af-bench 源在册——黄金串与源模板双向钉死 |
| `hostbridge_trait_is_object_safe` | trait 面按方案 §2.3 钉死：`&dyn` 可持 |

**黄金值来源（非人工目测）**：af-bench tb.rs:124-171 / swev.rs:55-99 的
format! 块**逐字复制**进推导程序（/tmp/p2-golden-derive/main.rs，rustc 实跑），
`\` 连续行折叠按 rustc 实际行为取值——含 local 分支 `--pids-limit 512` 后双
空格、tb build 代理参数后单空格等原样疵点；同程序证 `ssh` 与 `tb_ssh` 的包裹
产出**逐字节相等**（收敛为一的行为等价证据）。

## 3. 与方案的偏差（如实登记）

1. **对照形态降级为"黄金命令串 + 源文本比对"双层**。af-bench 的
   `TbEnv::setup`/`exec`/`swev::setup` 直调模块私有 `ssh()`/`tb_ssh()`，无法像
   P1 那样 `#[path]` 编译原文件注入假桥对拍（私有函数外部不可见，P1 已实测
   include! 吃不下文件头）。补偿：黄金串由源 format! 块逐字复制实跑得出 +
   `cmp_p2_templates_against_bench_source` 锁模板在册 + 真实现 local 分支真跑
   冒烟。属方案 §1.3"语句级对照清单"的可检验等价物，非偷工。
2. **初版 2 例黄金值推导错（已在 GREEN 前校正）**：`split_rc` 剥标记行保留其
   **前**换行（正文末行换行），初版推导漏计——`tb_exec_normal`（正文
   `out line\n\n` 而非 `out line\n`）与 `tb_exec_truncates`（20002 字节/elided
   8002/尾 5998+两换行，非 20001/8001/5999+一换行）。校正依据 = 实现为
   af-bench 逐字节原样搬入 + P1 既有黄金值 `split_rc("a\n__AF_RC=7\nstderr
   tail") → "a\n\nstderr tail"` 同款语义互证。测试先错后正，如实记。
3. **anolis ssh 分支无单测**：真 `ssh -o BatchMode=yes` 触网，属 L2 对拍面
   （方案 §4）；其命令构造由源文本比对钉死（模板原文在册断言）。
4. **真 docker 冒烟在册未跑**：`#[ignore]` 设计即"有 docker 才跑"；本任务
   资源纪律（不碰 docker）下未执行，运行判据留给具备 docker 的宿主。
5. **clippy useless_concat 一处**（重构提交 `584e462` 修正，黄金值折叠语义
   不变）；第三方 proc-macro-error2 v2.0.1 future-incompat 提示为 P1 已登记
   遗留（RESULT-P1.md §3-5），非本包新增。

## 4. 验证记录（本任务实跑，CARGO_TARGET_DIR=~/afbuild-phase2b，未占 af/target）

| 项 | 命令 | 结果 |
|---|---|---|
| 验收命令（任务书原文） | `cd /home/pan-ding/workspace/af-wt-phase2-p2 && CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline` | **全绿**（exit 0）：lib 14 + lifecycle 8 + p1_comparison 9 + p1_suite 15 + p2_bridge 12（1 ignored）+ doc 0 = **58 passed / 0 failed** |
| RED 基线 | 同上（RED 提交 dd3bc28 时点） | p2_bridge 10 红/2 过/1 ignore（过的两项：源文本比对与对象安全，不依赖桩）；既有 lib 14/lifecycle 8/p1_suite 15/p1_comparison 9 不受影响 |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo clippy -p af-execline --all-targets` | af-execline 自身 **0 警告**（exit 0）；仅第三方 proc-macro-error2 future-incompat 提示（§3-5） |
| fmt | `cargo fmt --check --all` | PASS（零 diff） |
| cargo deny | `cargo deny --offline check` | advisories / bans / licenses / sources 四类 ok |
| 只读约束 | `git diff 6ee05f8..HEAD --stat -- crates/af-bench crates/af-workorder crates/af-engine` | 空（零改动） |
| 迁移源一致性 | `diff` 主仓 af/crates/af-bench/src/{tb,swev}.rs vs 本工作树 | 逐字节一致（IDENTICAL，开工实测） |
| git 序列 | `git log --oneline` | dd3bc28（RED）→ ee0a6e0（GREEN）→ 584e462（重构）→ 本文件提交，红→绿成立 |
| 资源纪律 | 开工 `df -h /` | / 38G 可用（>15G 阈值）；构建只落 ~/afbuild-phase2b；零 docker、零 LLM 网络调用、零凭证消耗（真 local 冒烟仅 bash echo） |

## 5. 交接（P3 起点提示，未越界施工）

- `HostBridge`/`SshBridge` 已 pub 且对象安全：P3 `capture_diff`/eval 执行链、
  P4 `SwevExecuteUnit`、P5 TB verify 容器链都经 `&dyn HostBridge` 注入，
  FakeBridge 手法照用（tests/p2_bridge.rs 的 FakeBridge 可拷作模板）。
- `TbEnv` 字段保持 pub：P5 `verify`（tb.rs:226-292）搬入时挂同名 bridge 即可。
- `swev::setup` 返回的 `TbEnv.workdir="/testbed"`，与 P1 迁入的
  `bash_tool_decl(workdir)` 描述注入同源——P4 组装 bash 工具时两处必须同参。
- 黄金命令串中的代理/护栏/卫生组是**测量语义载荷**：任何人想"顺手修"双空格
  或统一 build 代理口径，先过 tests/p2_bridge.rs 黄金串与源文本比对两道门，
  并走独立任务包（本包纪律是迁移不是修复，方案 §1.3）。
