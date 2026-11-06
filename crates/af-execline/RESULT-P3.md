# RESULT-P3 —— af-bench 吸收 · 任务包 P3（判分单元与证据链）

> 身份：吸收施工员 P3（P3-F05），2026-10-24。工作树 `af-wt-phase2-p2`（分支
> `phase2-execline-p3`，基于 P2 完成点 286cc43），权威施工图
> `计划/研究组/af-bench吸收方案-20261024.md` §5-P3（§3 判分独立性四约束）。
> 施工形态：ADR-007 先红后绿再重构——RED `435e717`（验收测试 + judge/
> capture_diff/verify `unimplemented!()` 桩，p3_judge 3 红/1 过/1 ignore +
> p3_verify 7 红）→ GREEN `ac82065`（原样搬入转绿）→ 重构 `cfd0d04`
> （judge 失败三臂测试样板抽 helper，行为不变）。
> af-bench / af-workorder / af-engine 零改动
> （`git diff 286cc43..HEAD --stat -- crates/af-bench crates/af-workorder
> crates/af-engine` 输出为空，实测）；未写 计划/变更日志.md；未碰 P4-P6 内容
> （AgentRunner/SwevExecuteUnit/ExecContext 扩展/RAII guard 属 P4，
> driver 接线/TB verify 属 P5，薄壳化/影子对拍属 P6）。

## 1. 做了什么（源 af-bench swev.rs:行号 → 落位）

af-bench 源 = 本工作树 `crates/af-bench/src/`（开工 `diff` 实测与主仓
swev.rs/tb.rs/main.rs 三文件逐字节一致）。

| 搬入物 | af-bench 源 | af-execline 落位 | 迁移形态 |
|---|---|---|---|
| `JudgeLocator`（python/script/锚三字段） | 方案 §3.2-2/R2 设计；硬编码值 ← swev.rs:205-209 | `src/judge.rs:31` | 新增定位类型：路径参数化（默认=本机现产值），pub 字段可注入；`Clone` |
| `production()` | swev.rs:205-209 | `src/judge.rs:43` | 两路径逐字迁移；锚 = 现产 `a292547f3bfce825b0b44997fd5cc145a5aa7ee89d8533729716c0275950617c`（§3.1 实测，本任务 2026-10-24 复测一致）。历史锚（rsi-drill 冻结件 `0338566c2f457496037c70f16504b0cfe9f5286c55ee2d67963428b00e7fc16a`）按双锚口径用于回放对账：构造 Locator 指向冻结件 + 该锚即可，无内置构造器 |
| `check_fingerprint()`（指纹门） | 方案 §3.2-2 新增（af-bench 无门） | `src/judge.rs:56` | sha256(script) ≠ 声明锚 或脚本不可读 → EX3002 fail-loud（不降级、不跳判、不静默换脚本）；复用 `suite::swev::sha256_hex` 不另造哈希副本 |
| `invoke()`（判分子进程调用） | swev.rs:205-210 | `src/judge.rs:77` | 原样：`Command` 直指解释器、argv=[python, script, task_json, log, diff]（判分脚本恰收三位置参数）、无 env 注入、不经 wrapper shell——判分调用唯一收敛点（§3.2-1）；stdout/stderr/退出码原样交 verify 解读 |
| `capture_diff` | swev.rs:102-117 | `src/suite/swev.rs:186` | 原样（`tb_ssh()` 换 `bridge.run()`；命令串含 `diff' ` 后尾随空格、超时 60、`取 diff 失败:`/`写 agent.diff 失败` 文案逐字节不变） |
| `verify` | swev.rs:145-258 | `src/suite/swev.rs:215` | 原样主干（指纹门前置 → capture_diff 接 bridge → eval.sh 写入 → 容器内执行 stdin/stdout 文件管道 → 124 落 `EVAL_TIMEOUT_KILLED` 标记 → **标记后**算 sha256 → judge::invoke → resolved/log_parsed 透传与 note 前缀拼装）；三处迁移注记见 §3-1/3-2/3-3 |
| `sha256_hex` | swev.rs:261-266 | `src/suite/swev.rs:346` | 原样；私有 → `pub(crate)`（judge.rs 指纹门复用；落位 suite::swev 系方案 §1.1 表指定 home） |
| `ErrorCode::JudgeFingerprintMismatch` | —（新错误码） | `src/error.rs:31,47,63,100` | EX3002，数字只增不改；码表锁定测试更新 |

**判分三件套契约不变量**（方案 §1.1）：`task_json`（判分器全字段来源，
verify 的 `task_json_path` 参数直通 judge argv）/ `eval.log`（含超时标记行，
哈希在标记后算）/ `agent.diff`（capture_diff 落 work_dir）——三者路径与
字节语义不动；judge 只读三件套、写不回。判分输入输出接口（swev-judge.py
argv 三参 + stdout JSON）不动。

**未搬（按方案范围纪律）**：`AgentRunner`/`SwevExecuteUnit`/ExecContext/
ExecOutput 扩展/RAII cleanup guard（P4）、bench_order→driver 引擎 op 接线与
attempt 目录驱动层分配（P5）、`TbEnv::verify` TB 容器链（P5）、af-bench
薄壳化与影子对拍 L0/L1/L2（P6）。

## 2. 测试清单（p3_judge 5 例 + p3_verify 7 例：11 跑全绿 + 1 ignore）

方案 §5-P3 验收测试草案逐项落位：

| 测试 | 覆盖（草案条目 → §3.2 约束） |
|---|---|
| `judge_invoke_argv_contract_no_env_injection_no_wrapper_shell` | **argv 契约**：恰 [python, script, task_json, log, diff] 五元素（判分脚本恰收三位置参数）；对抗路径（空格/引号/$/;/反斜杠）逐字节穿透；无 env 注入（子进程原始 environ 与测试进程全等，零豁免项）；不经 wrapper shell（假 python 父进程非 bash/ssh/timeout 等包装层）——§3.2-1 三条款全行为证据 |
| `judge_fingerprint_gate_fail_loud_ex3002` | **指纹门**：锚匹配放行；造改字节脚本副本（草案原话）→ EX3002、实测/声明双指纹进文案、fail-loud 措辞；脚本缺席=无法验真同样拒跑——§3.2-2 |
| `judge_production_locator_is_current_machine_values` | production 默认值 = swev.rs:205-209 硬编码迁移 + 现产锚字面锁定 |
| `cmp_p3_judge_templates_against_bench_source` | 源文本比对：判分器定位/argv 形状（.arg 链）/eval 桥接模板/`EVAL_TIMEOUT_KILLED` 标记/note 拼装全部模板/三件套路径拼装/`Sha256::digest` 原文必须在 af-bench swev.rs 在册——实现改构造即失配、af-bench 模板漂移即报警，双向钉死 |
| `real_judge_fixture_trio_resolved`（`#[ignore]`） | **样例三件套 fixture 真跑 venv judge**：crate 内 fixture（astropy__astropy-12907 裁剪件），真 venv + 现产脚本 + 现产锚；两份日志仅差 f2p 的 PASSED/FAILED 一词 → resolved false/true 随日志翻转（判定值确出自 judge stdout，非旁路）；venv/脚本缺席打印说明跳过不假绿。**本任务已实跑通过**（`-- --ignored`，双判分 2.0s，exit 0） |
| `capture_diff_command_artifact_and_error_contract` | 取证命令串（含尾随空格）+ 超时 60 + agent.diff 落盘字节 + `取 diff 失败`/`写 agent.diff 失败` 两错误路径 |
| `verify_timeout_marker_before_hash_and_note_stack` | **顺序断言**（§3.2-4）：eval 退出 124 → 标记行落日志末尾 → 之后才算 sha256（返回指纹 == 对最终日志字节独立重算，哈希先于标记即失配）；另钉 eval.sh 逐字节、eval.sh 经 stdin 喂容器（假 docker 吃到的 stdin 落证全等）、note 前缀栈（超时击杀→log_parsed=false→judge 原文）、verify→judge argv 接线（三件套路径拼装） |
| `verify_normal_flow_and_log_parsed_prefix_matrix` | 退出非 124：无标记行、无超时前缀、sha=无标记日志重算；**log_parsed=false → `[log_parsed=false] ` 前缀透传**（swev.rs:219-222），true/缺席 → 无前缀 |
| `verify_judge_failure_paths_match_bench_messages` | judge 失败三臂原样文案：退出非零（`judge 退出 <status>: <stderr 前 300>`）/ stdout 非 JSON（`judge 输出不是 JSON: …`）/ 拉起失败（`judge 启动失败: …`）；三臂保留 eval_sha256（日志字节可对账） |
| `verify_eval_sh_write_failure_short_circuits` | `写 eval.sh 失败` 短路（目录占位）：eval 执行与判分都不跑，eval_sha256=None |
| `verify_capture_diff_failure_short_circuits` | capture_diff 桥接失败透传 + eval_sha256=None（D-20 项 2 早期失败语义） |
| `verify_fingerprint_gate_blocks_before_any_work` | **拒跑行为证据**：指纹失配 → EX3002 进 note、桥零调用、run 目录三件套零文件——不只是错误码存在 |

**fixture 设计与实证**（tests/fixtures/p3-judge/）：`task.json` 取自真实任务包
`/tmp/swev-s1/astropy__astropy-12907.json`（SWE-bench Verified 公开数据）裁剪
——真实 repo/version/base_commit/test_patch（swebench 4.1 规格表内，
`make_test_spec` **零出网**：astropy 不在 env_yml/reqs 网络拉取集合，本任务
以假代理阻断网络实测判分全程无请求），fail_to_pass/pass_to_pass 各一题；
`eval.log`/`eval-ok.log` 含 Start/End Test Output 标记与 pytest 摘要行。
判分确定性双跑实证（假代理下 1s/次）。

**注入手法**（零 docker 守护进程、零 ssh、零网络、零模型——资源纪律）：
FakeBridge（P2 手法照用）+ 假 docker（PATH 首段注入，按容器名分派退出码
124/0，stdin 落证）+ 假判分 venv（假 python 记 argv 后回 JSON）。

## 3. 与方案的偏差（如实登记）

1. **指纹门接线点提前**。方案 §3.2-2 把门定在"单元装载时"（P4 的
   SwevUnitSpec）；P3 尚无单元，门接在 `suite::swev::verify` 入口（P3 可执行
   流的最早点，拒跑语义=指纹失配时桥零调用、run 目录零文件，有行为测试）。
   P4 装载门仍会加——双重防线，脚本 sha256 ~10KB 微秒级、双算无害。
2. **eval 执行段不走 bridge（"接 bridge"的边界解读）**。方案 P3 目标行写
   "搬入并接 bridge"；但 HostBridge trait 面按方案 §2.3 钉死
   （`run(host, script, timeout) → (bool, String)`），表达不了 stdin（eval.sh
   喂容器 bash）与 stdout（落 eval.log）的文件管道、以及外层 stderr 丢弃
   （容器内 `2>&1` 已合并进日志，桥接层再并入会把 ssh 噪声写进判分日志改变
   判分输入字节）。af-bench 原实现该段（swev.rs:169-186）本就直造 Command
   不经 `tb_ssh`——"接 bridge"适用于原经 `tb_ssh` 的 capture_diff（方案 P3
   依赖注记"取证命令走 bridge 处需 P2"同款解读）。语句逐字对照搬运，仅局部
   变量 `bridge` 更名 `bridge_script`（避开本函数新增的 bridge 参数），超时
   包裹模板串逐字节不变（cmp 在册）。
3. **judge 调用对照形态**。判分调用内联于 af-bench verify、路径硬编码、私有
   不可注入——无法像 P1 那样 `#[path]` 编译原文件对拍。补偿三层：假判分 venv
   行为断言（真 Command 真子进程，argv/env/父进程实测）+ 源文本比对（模板
   在册）+ 真 venv fixture（现产脚本真跑）。属方案 §1.3"语句级对照清单"的
   可检验等价物。
4. **argv 契约测试的假解释器用 python3 而非 bash**。bash 启动即自维护
   SHLVL/_/PWD/OLDPWD，污染"子进程环境与父进程全等"断言；python3 不改动
   继承 environ，dump 出的即 execve 传入的原始环境——全等断言零豁免项
   （RED→GREEN 期间的测试修正，非实现放水：初版用 bash 差四个 shell 自维护
   变量失配）。测试环境因此多一个 /usr/bin/python3 依赖（本机在位）。
5. **RED→GREEN 期间另两处测试修正**（测试自身缺陷）：run 目录与假 venv 目录
   tag 撞名致 `temp_root` 的 remove_dir_all 互删对方文件（eval.sh 占位被
   venv 重建抹掉、假判分脚本被 run 目录重建抹掉→指纹门误触）；假 docker
   redirect 目标写在单引号内 `$3` 不展开。均已修（`-run` 后缀分离 tag、
   双引号包裹），如实记。
6. **`&text[..text.len().min(200)]` 多字节边界 panic 风险原样携带**
   （swev.rs:238 judge 输出非 JSON 臂）：judge stdout 非合法 JSON 且含多字节
   字符恰在 200 字节边界时 af-bench 原版即会 panic。迁移纪律不顺手修
   （修了就不是迁移，同 R8 口径），登记为后续独立决定。
7. **真 local docker 冒烟未跑**：P3 无新增 docker 冒烟（P2 既有 `#[ignore]`
   项维持）；本任务资源纪律禁碰 docker，eval 执行链以 PATH 注入假 docker
   行为等效覆盖（timeout 包裹/exit 124/stdin/stdout 管道全实测，唯一不可
   假的是真 dockerd 行为，留 L1/L2 对拍面）。
8. **clippy 零新增**；第三方 proc-macro-error2 v2.0.1 future-incompat 提示
   为 P1 已登记遗留（RESULT-P1.md §3-5），非本包新增。

## 4. 验证记录（本任务实跑，CARGO_TARGET_DIR=~/afbuild-phase2b，未占 af/target）

| 项 | 命令 | 结果 |
|---|---|---|
| 验收命令（任务书原文） | `cd /home/pan-ding/workspace/af-wt-phase2-p2 && CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline` | **全绿**（exit 0）：lib 14 + lifecycle 8 + p1_comparison 9 + p1_suite 15 + p2_bridge 12（1 ignored）+ p3_judge 4（1 ignored）+ p3_verify 7 + doc 0 = **69 passed / 0 failed** |
| 真 venv fixture | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline --test p3_judge -- --ignored` | **通过**（exit 0，1 passed，2.0s：现产指纹门过锚 + 双判分 resolved false→true 翻转实测） |
| RED 基线 | 同验收命令（RED 提交 435e717 时点） | p3_judge 3 红/1 过/1 ignore（过的 cmp 源文本比对不依赖桩）+ p3_verify 7 红；既有 lib 14/lifecycle 8/p1_comparison 9/p1_suite 15/p2_bridge 12（1 ignore）全绿不受影响 |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo clippy -p af-execline --all-targets` | af-execline 自身 **0 警告**（exit 0）；仅第三方 proc-macro-error2 future-incompat 提示（§3-8） |
| fmt | `cargo fmt --check --all` | PASS（零 diff，exit 0） |
| cargo deny | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo deny --offline check` | advisories / bans / licenses / sources 四类 ok（exit 0） |
| 只读约束 | `git diff 286cc43..HEAD --stat -- crates/af-bench crates/af-workorder crates/af-engine` | 输出为空（0 字节，零改动） |
| 迁移源一致性 | `diff` 主仓 af/crates/af-bench/src/{swev,tb,main}.rs vs 本工作树 | 逐字节一致（IDENTICAL，开工实测） |
| fixture 零出网 | `env https_proxy=http://127.0.0.1:9 http_proxy=http://127.0.0.1:9 /home/pan-ding/swev-venv/bin/python tools/swev-judge.py <fixture 三件套>` | 判分成功（假代理阻断下无任何网络请求）：eval.log→resolved=false/log_parsed=true，eval-ok.log→resolved=true/log_parsed=true，确定性复跑一致 |
| 现产判分指纹 | `sha256sum /home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py` | `a292547f3bfce825…0617c`（与 production() 锚字面一致）；冻结件 `0338566c…7fc16a` 在册双锚 |
| git 序列 | `git log --oneline` | 435e717（RED）→ ac82065（GREEN）→ cfd0d04（重构）→ 本文件提交，红→绿→重构成立 |
| 资源纪律 | 开工/收尾 `df -h /` | 开工 29G 可用 → 收尾 20G 可用（>15G 阈值；消耗=~/afbuild-phase2b 构建缓存+机器在飞负载）；零 docker 守护进程接触、零 LLM 调用、零凭证消耗（假判分/假 docker/fixture 全离线）；会话末段 shell 记账偶发"磁盘配额"写错误（/tmp tmpfs 81%，他因在飞进程），不影响任何提交与验证结果（每项均复核实测） |

## 5. 交接（P4 起点提示，未越界施工）

- `JudgeLocator`（Clone、pub 字段）即 §2.2 SwevUnitSpec 的"判分器定位与
  指纹锚"槽位；P4 组 spec 时把 production() 或对账变体放进去，并在**单元
  装载时**调 `check_fingerprint()`（verify 入口的门保留为第二道防线）。
- `verify(bridge, judge, host, env, task, work_dir, task_json_path)` 七参即
  P4 `SwevExecuteUnit::execute` 重组的素材底座；§2.4 的 RAII cleanup guard
  （Drop → `docker rm -f`）与 setup_failed/run_failed 行语义是 P4 增量，
  verify 本体不再改签名。
- `sha256_hex` 为 pub(crate)：P4 record 模块的 `eval_sha256` 直接走 verify
  返回值（同一份字节指纹，勿另行重算造成双源）。
- argv 契约/指纹门/顺序断言三组测试是判分面的常驻守卫：任何人改 judge 调用
  形态（加 wrapper、注 env、换脚本路径）、动"标记→哈希→判分"顺序、或现产
  脚本漂移（fixture --ignored 测试过锚失败即报警），先过这三道门再谈理由。
- §3-6 的 200 字节切片 panic 风险在册：P4+ 如要修，走独立任务包（迁移期
  不修的纪律与 R8 同口径）。
