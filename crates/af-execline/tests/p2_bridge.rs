//! P2 宿主桥接抽象验收测试（af-bench 吸收方案 §5-P2 草案全项）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，`unimplemented!()` 桩下必红。
//!
//! 对照形态：af-bench 的 `TbEnv::setup`/`exec`/`swev::setup` 直调模块私有
//! `ssh()`/`tb_ssh()`，无法像 P1 那样 `#[path]` 编译原文件注入假桥对拍——对照
//! 改为三层钉死：
//! 1. **FakeBridge 黄金命令串**（本文件主体）：断言迁移实现经桥发出的命令串、
//!    超时与错误文案逐字节等于 af-bench 构造值。黄金值非人工目测：af-bench
//!    tb.rs:124-171 / swev.rs:55-99 的 format! 块**逐字复制**进推导程序实跑而得
//!    （`\` 连续行折叠按 rustc 实际行为，含 local 分支 `--pids-limit 512` 后
//!    双空格等原样疵点，迁移不顺手修），推导记录见 RESULT-P2.md。
//! 2. **源文本比对**（cmp_p2_* 系列）：黄金命令串的模板原文必须在 af-bench
//!    源文件中在册——实现若改命令构造，黄金串即失配；af-bench 模板若漂移，
//!    断言即报警。
//! 3. **真实现 local 冒烟**（非 ignore）：`SshBridge` local 分支真跑 bash echo
//!    （零 docker/ssh/网络，与 P1 resolve_key printf 测试同类）。
//!
//! `#[ignore]` 真 local docker 冒烟 1 例按方案草案在册（有 docker 才跑）。
//! 黄金常量用 `\` 连续行书写（折叠后单空格连接、行尾多空格保留），与 af-bench
//! 源码同一书写形态。

use af_execline::hostbridge::{HostBridge, SshBridge};
use af_execline::suite::swev::{self, SwevTask};
use af_execline::suite::tb::{TbEnv, TbTask};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;

type BridgeResult = Result<(bool, String), String>;

// ---- FakeBridge：记录 (host, script, timeout_sec)，按序弹脚本化响应 ----

#[derive(Default)]
struct FakeBridge {
    calls: RefCell<Vec<(String, String, u32)>>,
    scripted: RefCell<VecDeque<BridgeResult>>,
}

impl FakeBridge {
    fn new(scripted: Vec<BridgeResult>) -> Self {
        Self {
            calls: RefCell::default(),
            scripted: RefCell::new(scripted.into()),
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.borrow().clone()
    }
}

impl HostBridge for FakeBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> BridgeResult {
        self.calls
            .borrow_mut()
            .push((host.to_string(), script.to_string(), timeout_sec));
        self.scripted
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok((true, String::new())))
    }
}

fn swev_task() -> SwevTask {
    SwevTask {
        instance_id: "astropy__astropy-12907".into(),
        repo: "astropy/astropy".into(),
        problem_statement: "Fix the sampler".into(),
        image: "sweb.eval.x86_64.astropy-12907".into(),
        eval_script: String::new(),
        hints_text: String::new(),
    }
}

fn tb_task() -> TbTask {
    TbTask {
        task_id: "hello-world".into(),
        prompt: "do the thing".into(),
    }
}

/// 取 Err 文案（TbEnv 无 Debug derive，unwrap_err 不可用）。
fn as_err<T>(result: Result<T, String>) -> String {
    result.err().unwrap_or_else(|| panic!("期望 Err，实得 Ok"))
}

// ---- swev setup（方案 §5-P2 草案：护栏 6g/512、anolis 三代理变量 local 不含、
// PAGER/TQDM 卫生组、前置 docker rm -f）----

/// 黄金值 ← af-bench swev.rs:63-85 format! 块逐字复制实跑（anolis 分支）。
const SWEV_RUN_ANOLIS: &str = "docker run -d --name afbench-swev-astropy-astropy-12907-glm-cp \
--memory 6g --pids-limit 512 \
-e HTTPS_PROXY=http://172.17.0.1:8123 -e HTTP_PROXY=http://172.17.0.1:8123 \
-e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
-e PAGER=cat -e MANPAGER=cat -e LESS=-R \
-e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1 \
sweb.eval.x86_64.astropy-12907 sleep infinity";

/// local 分支（swev.rs:63-64 proxy_env=""）：`--pids-limit 512` 后是**双空格**
/// （空 proxy_env 两侧各留一空格，`\` 折叠后的原样疵点，迁移不顺手修）。
const SWEV_RUN_LOCAL: &str = "docker run -d --name afbench-swev-astropy-astropy-12907-glm-cp \
--memory 6g --pids-limit 512  \
-e PAGER=cat -e MANPAGER=cat -e LESS=-R \
-e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1 \
sweb.eval.x86_64.astropy-12907 sleep infinity";

#[test]
fn swev_setup_anolis_builds_guardrail_proxy_hygiene_command() {
    let bridge = FakeBridge::default();
    let env = swev::setup(&bridge, "anolis", &swev_task(), "glm-cp").unwrap();
    // 容器名：instance_id 的 `__` 折成 `-`（swev.rs:56-59），workdir=/testbed。
    assert_eq!(env.container, "afbench-swev-astropy-astropy-12907-glm-cp");
    assert_eq!(env.task, "astropy__astropy-12907");
    assert_eq!(env.host, "anolis");
    assert_eq!(env.workdir, "/testbed");
    assert_eq!(env.workdir(), "/testbed");

    let calls = bridge.calls();
    assert_eq!(calls.len(), 2, "恰好两次桥调用：前置 rm + docker run");
    // 前置 docker rm -f（swev.rs:60，超时 30）。
    assert_eq!(
        calls[0],
        (
            "anolis".to_string(),
            "docker rm -f afbench-swev-astropy-astropy-12907-glm-cp".to_string(),
            30
        )
    );
    // docker run（swev.rs:77-85，超时 120）：护栏 6g/512 + 三代理变量 + 卫生组。
    assert_eq!(
        calls[1],
        ("anolis".to_string(), SWEV_RUN_ANOLIS.to_string(), 120)
    );
    let run = &calls[1].1;
    assert!(run.contains("--memory 6g --pids-limit 512"), "{run}");
    assert!(
        run.contains(
            "-e HTTPS_PROXY=http://172.17.0.1:8123 -e HTTP_PROXY=http://172.17.0.1:8123 \
             -e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10"
        ),
        "{run}"
    );
    assert!(
        run.contains("-e PAGER=cat -e MANPAGER=cat -e LESS=-R"),
        "{run}"
    );
    assert!(
        run.contains("-e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1"),
        "{run}"
    );
    assert!(
        run.ends_with("sweb.eval.x86_64.astropy-12907 sleep infinity"),
        "{run}"
    );
}

#[test]
fn swev_setup_local_omits_proxy_env() {
    let bridge = FakeBridge::default();
    let env = swev::setup(&bridge, "local", &swev_task(), "glm-cp").unwrap();
    assert_eq!(env.host, "local");
    let calls = bridge.calls();
    assert_eq!(calls.len(), 2);
    let run = &calls[1].1;
    assert_eq!(run, SWEV_RUN_LOCAL);
    // local 分支不含任何代理变量（swev.rs:63-64），护栏与卫生组仍在。
    assert!(!run.contains("PROXY"), "{run}");
    assert!(run.contains("--memory 6g --pids-limit 512"), "{run}");
    assert!(
        run.contains("-e PAGER=cat -e MANPAGER=cat -e LESS=-R"),
        "{run}"
    );
}

#[test]
fn swev_setup_run_failure_error_messages() {
    // docker run 返回非零 → "起容器失败: <log 前 400 字符>"（swev.rs:93-96，
    // 注意是 chars().take(400) 头截断，与 tb 的 tail 尾截断不同）。
    let bridge = FakeBridge::new(vec![
        Ok((true, String::new())),    // rm
        Ok((false, "x".repeat(500))), // run
    ]);
    let err = as_err(swev::setup(&bridge, "anolis", &swev_task(), "glm-cp"));
    assert_eq!(err, format!("起容器失败: {}", "x".repeat(400)));

    // 桥接启动失败原样透传（swev.rs:97 `Err(err) => Err(err)`）。
    let bridge = FakeBridge::new(vec![
        Ok((true, String::new())),
        Err("命令桥接启动失败: No such file or directory".into()),
    ]);
    assert_eq!(
        as_err(swev::setup(&bridge, "anolis", &swev_task(), "glm-cp")),
        "命令桥接启动失败: No such file or directory"
    );
}

// ---- TbEnv 生命周期（setup/exec/cleanup 挂 bridge；tb.rs:124-171, 186-222, 294-296）----

/// 黄金值 ← af-bench tb.rs:131-139 build format! 块逐字复制实跑。
const TB_BUILD: &str = "docker build -q --network host \
--build-arg HTTPS_PROXY=http://127.0.0.1:8123 \
--build-arg HTTP_PROXY=http://127.0.0.1:8123 \
--build-arg NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
-t afbench-tbenv-hello-world /data/bench/tb/hello-world/environment";

/// 黄金值 ← af-bench tb.rs:150-159 run format! 块逐字复制实跑。
const TB_RUN: &str = "docker run -d --name afbench-hello-world-glm-cp \
--memory 2g --pids-limit 256 \
-e HTTPS_PROXY=http://172.17.0.1:8123 -e HTTP_PROXY=http://172.17.0.1:8123 \
-e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
-e PAGER=cat -e MANPAGER=cat -e LESS=-R \
-e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1 \
afbench-tbenv-hello-world sleep infinity";

#[test]
fn tb_setup_and_cleanup_command_sequence() {
    let bridge = FakeBridge::default();
    let env = TbEnv::setup(&bridge, "anolis", &tb_task(), "glm-cp").unwrap();
    assert_eq!(env.container, "afbench-hello-world-glm-cp");
    assert_eq!(env.task, "hello-world");
    assert_eq!(env.host, "anolis");
    assert_eq!(env.workdir, "/app");
    assert_eq!(env.workdir(), "/app");

    let calls = bridge.calls();
    assert_eq!(
        calls.len(),
        3,
        "恰好三次桥调用：build + 前置 rm + docker run"
    );
    // build：代理 build-args（127.0.0.1:8123 构建期口径）+ 超时 1800。
    assert_eq!(calls[0], ("anolis".to_string(), TB_BUILD.to_string(), 1800));
    assert!(calls[0]
        .1
        .contains("--build-arg HTTPS_PROXY=http://127.0.0.1:8123"));
    assert!(calls[0]
        .1
        .ends_with("-t afbench-tbenv-hello-world /data/bench/tb/hello-world/environment"));
    // 前置 rm（tb.rs:145，超时 30）。
    assert_eq!(
        calls[1],
        (
            "anolis".to_string(),
            "docker rm -f afbench-hello-world-glm-cp".to_string(),
            30
        )
    );
    // run：TB 护栏 2g/256 + 容器级代理 + 卫生组，超时 60。
    assert_eq!(calls[2], ("anolis".to_string(), TB_RUN.to_string(), 60));
    assert!(calls[2].1.contains("--memory 2g --pids-limit 256"));

    // cleanup：docker rm -f，超时 60（tb.rs:294-296）。
    env.cleanup(&bridge);
    let calls = bridge.calls();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[3],
        (
            "anolis".to_string(),
            "docker rm -f afbench-hello-world-glm-cp".to_string(),
            60
        )
    );
}

#[test]
fn tb_setup_failure_error_messages() {
    // build 失败 → "build {image} 失败: <tail 800>"（tb.rs:142-144 尾截断）。
    let bridge = FakeBridge::new(vec![Ok((false, "0123456789".repeat(100)))]);
    let err = as_err(TbEnv::setup(&bridge, "anolis", &tb_task(), "glm-cp"));
    let expected_tail = "0123456789".repeat(100)[200..].to_string();
    assert_eq!(
        err,
        format!("build afbench-tbenv-hello-world 失败: {expected_tail}")
    );

    // 起容器失败 → "起容器 {container} 失败: <tail 400>"（tb.rs:162-164）。
    let bridge = FakeBridge::new(vec![
        Ok((true, String::new())),                    // build
        Ok((true, String::new())),                    // rm
        Ok((false, "port already allocated".into())), // run
    ]);
    assert_eq!(
        as_err(TbEnv::setup(&bridge, "anolis", &tb_task(), "glm-cp")),
        "起容器 afbench-hello-world-glm-cp 失败: port already allocated"
    );
}

/// exec 桥调用构造（tb.rs:187-191）：docker exec + cd 前缀 + `; echo __AF_RC=$?`，
/// 超时 180；正常路径剥标记行、exit=N 落末行。
#[test]
fn tb_exec_normal_splits_marker_and_appends_exit_line() {
    let bridge = FakeBridge::new(vec![Ok((true, "out line\n__AF_RC=0\n".into()))]);
    let env = TbEnv::setup(&FakeBridge::default(), "anolis", &tb_task(), "glm-cp").unwrap();
    let out = env.exec(&bridge, "ls");
    // 剥标记行保留其前后换行（前换行属正文末行，P1 split_rc 黄金值
    // "a\n\nstderr tail" 同款语义）：正文 "out line\n\n"，再补 "\nexit=0"。
    assert_eq!(out, "out line\n\n\nexit=0");
    let calls = bridge.calls();
    assert_eq!(calls.len(), 1, "exec 只发一条桥命令");
    assert_eq!(
        calls[0],
        (
            "anolis".to_string(),
            "docker exec afbench-hello-world-glm-cp bash -lc 'cd /app && ls'; echo __AF_RC=$?"
                .to_string(),
            180
        )
    );
}

/// 外层超时（无 __AF_RC 标记）→ mini-swe-agent 同源教学文案 + exit=124（tb.rs:201-205）。
#[test]
fn tb_exec_timeout_without_marker_teaches_and_exits_124() {
    let bridge = FakeBridge::new(vec![Ok((true, "partial output\n".into()))]);
    let env = TbEnv::setup(&FakeBridge::default(), "anolis", &tb_task(), "glm-cp").unwrap();
    let out = env.exec(&bridge, "sleep 999");
    assert_eq!(
        out,
        concat!(
            "partial output\n\n[harness] The command was killed after exceeding the 180s timeout \
(no exit code was reported). Partial output (if any) is above. Re-run with a different approach: \
pipe input or use non-interactive flags (-y / -n / DEBIAN_FRONTEND=noninteractive), wrap long \
work in an explicit `timeout <secs> ...`, and avoid commands that never exit on their own \
(servers, watch loops, pagers).\nexit=124"
        )
    );
}

/// 桥接启动失败 → "基建瞬断、可重试"文案 + exit=127（tb.rs:194-199），运行不死。
#[test]
fn tb_exec_bridge_failure_reports_transient_infrastructure() {
    let bridge = FakeBridge::new(vec![Err("命令桥接启动失败: ssh: connect refused".into())]);
    let env = TbEnv::setup(&FakeBridge::default(), "anolis", &tb_task(), "glm-cp").unwrap();
    let out = env.exec(&bridge, "ls");
    assert_eq!(
        out,
        "[harness] The execution bridge failed before the command could produce output \
(transient infrastructure error: 命令桥接启动失败: ssh: connect refused). This is not a task \
failure — retry the command.\nexit=127"
    );
}

/// 输出截断：CAP 12_000，保头尾各 6_000，中间落 elided 标记与换窄命令指引
/// （tb.rs:206-215）。
#[test]
fn tb_exec_truncates_at_cap_12000() {
    let raw = format!("{}\n__AF_RC=0\n", "a".repeat(20_000));
    let bridge = FakeBridge::new(vec![Ok((true, raw))]);
    let env = TbEnv::setup(&FakeBridge::default(), "anolis", &tb_task(), "glm-cp").unwrap();
    let out = env.exec(&bridge, "cat big");
    // 剥标记行保留前后换行：text = 20_000 a + "\n\n"（20_002 字节）
    // → 头 6_000 + elided 8_002 + 尾 6_000（5_998 个 a + 两换行），再补 "\nexit=0"。
    let expected = format!(
        "{}\n...[output truncated: 8002 bytes elided. Re-run with a narrower command: pipe \
through head/tail/sed -n, grep for specific patterns, or redirect output to a file and read it \
in parts.]\n{}\n\n\nexit=0",
        "a".repeat(6000),
        "a".repeat(5998)
    );
    assert_eq!(out, expected);
    assert!(
        out.len() < 12_500,
        "截断后总长 {} 应显著低于原 20_002",
        out.len()
    );
}

// ---- 真实现 local 冒烟（零 docker/ssh/网络；timeout+bash 直执，stderr 合并）----

#[test]
fn ssh_bridge_local_branch_runs_bash_merges_stderr_and_status() {
    let bridge = SshBridge;
    // stdout/stderr 合并语义（tb.rs:55-62）：非空 stdout 与 stderr 之间补一个换行。
    let (ok, text) = bridge.run("local", "echo out; echo err >&2", 10).unwrap();
    assert!(ok);
    assert_eq!(text, "out\n\nerr\n");
    // 非零退出 → status=false；无输出时文本为空。
    let (ok, text) = bridge.run("local", "exit 3", 10).unwrap();
    assert!(!ok);
    assert_eq!(text, "");
    // 含单引号的脚本必须经 shell_quote 正确转义穿透两层 bash -c——转义错一丝
    // 即 bash 语法错（ok=false），此断言钉死包裹构造（timeout N bash -c '...'）。
    let (ok, text) = bridge.run("local", "echo \"it's\"", 10).unwrap();
    assert!(ok);
    assert_eq!(text, "it's\n");
}

/// 真 local docker 冒烟（方案 §5-P2 草案：有 docker 才跑，默认 ignore）。
/// 运行：`cargo test -p af-execline --test p2_bridge -- --ignored`。
/// 无 docker / 无 busybox 镜像时打印说明后跳过（不拉镜像、不触网，不假绿不误红）。
#[test]
#[ignore = "真 local docker 冒烟：需本机 docker 守护进程（cargo test -- --ignored 运行）"]
fn smoke_real_local_docker_via_bridge() {
    let bridge = SshBridge;
    let (ok, text) = bridge
        .run("local", "docker version --format '{{.Server.Version}}'", 30)
        .unwrap();
    if !ok {
        eprintln!("跳过：本机 docker 不可用（docker version 失败: {text}）");
        return;
    }
    println!("docker server version: {}", text.trim());

    // busybox 镜像在本机存在时，走一遍最小容器生命周期（rm 前置 → run 带护栏 →
    // exec → rm 收尾）；镜像缺席则只完成上面的版本冒烟。
    let (has_image, _) = bridge
        .run(
            "local",
            "docker image inspect busybox >/dev/null 2>&1 && echo yes",
            30,
        )
        .unwrap();
    if !has_image {
        eprintln!("跳过容器生命周期段：本机无 busybox 镜像（不拉取，避免触网）");
        return;
    }
    let name = "afexl-p2-smoke";
    let _ = bridge.run("local", &format!("docker rm -f {name}"), 30);
    let (ok, log) = bridge
        .run(
            "local",
            &format!(
                "docker run -d --name {name} --memory 64m --pids-limit 64 busybox sleep infinity"
            ),
            60,
        )
        .unwrap();
    assert!(ok, "起冒烟容器失败: {log}");
    let (ok, text) = bridge
        .run("local", &format!("docker exec {name} echo ok"), 30)
        .unwrap();
    assert!(ok);
    assert_eq!(text.trim(), "ok");
    let _ = bridge.run("local", &format!("docker rm -f {name}"), 30);
}

// ---- 源文本比对：黄金命令串的模板原文必须在 af-bench 源在册 ----

fn bench_src(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("读 {} 失败: {err}", path.display()))
}

/// 桥接包裹 + swev/tb setup/exec 的命令模板与失败文案原文在册（tb.rs / swev.rs）。
/// 黄金串由这些模板 + 已对照的护栏常量（P1 cmp_guardrail_constants）决定——
/// 实现改动命令构造即与黄金串失配，模板漂移即与本断言失配，双向钉死。
#[test]
fn cmp_p2_templates_against_bench_source() {
    let tb = bench_src("../af-bench/src/tb.rs");
    let swev = bench_src("../af-bench/src/swev.rs");

    // 桥接包裹模板：tb.rs:43 与 swev.rs:269-271 同构（收敛为一的依据）。
    assert!(
        tb.contains(r#"format!("timeout {timeout_sec} bash -c {}", shell_quote(script))"#),
        "tb.rs 桥接包裹模板漂移"
    );
    assert!(swev.contains(r#""timeout {timeout_sec} bash -c '{}'"#));
    assert!(swev.contains(r#"script.replace('\'', "'\\''")"#));

    // swev setup：容器名 / 前置 rm / 护栏 / 卫生组 / 代理分支 / 头截断 400。
    assert!(swev.contains(r#""afbench-swev-{}-{profile}""#));
    assert!(swev.contains(".replace(\"__\", \"-\")"));
    assert!(swev.contains("docker rm -f {container}"));
    assert!(swev.contains("--memory {mem} --pids-limit {pids}"));
    assert!(swev.contains("-e PAGER=cat -e MANPAGER=cat -e LESS=-R"));
    assert!(swev.contains("-e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1"));
    assert!(swev.contains("-e HTTPS_PROXY=http://172.17.0.1:8123"));
    assert!(swev.contains(".chars().take(400)"));

    // tb setup/exec/cleanup：build 代理口径 / 护栏 / exec 脚本 / rm 模板 / 截断与文案。
    assert!(
        tb.contains("--memory {TB_CONTAINER_MEM} --pids-limit {TB_CONTAINER_PIDS}"),
        "tb.rs 护栏模板漂移"
    );
    assert!(tb.contains("--build-arg HTTPS_PROXY=http://127.0.0.1:8123"));
    assert!(tb.contains("-e HTTPS_PROXY=http://172.17.0.1:8123"));
    assert!(tb.contains(r#""docker exec {} bash -lc {}; echo __AF_RC=$?""#));
    assert!(tb.contains("docker rm -f {container}"));
    assert!(tb.contains("docker rm -f {}"));
    assert!(tb.contains("tail(&log, 800)"));
    assert!(tb.contains("tail(&log, 400)"));
    assert!(tb
        .contains("[harness] The execution bridge failed before the command could produce output"));
    assert!(tb.contains("The command was killed after exceeding the 180s timeout"));
    assert!(tb.contains(
        "bytes elided. Re-run with a narrower command: pipe through head/tail/sed -n, grep \
             for specific patterns, or redirect output to a file and read it in parts.]"
    ));
}

/// trait 面按方案 §2.3 钉死：对象安全（`&dyn` 可持），签名三参一返回。
/// （每个 FakeBridge/SshBridge 测试都在行使对象安全，此处显式固化防退化。）
#[test]
fn hostbridge_trait_is_object_safe() {
    let fake = FakeBridge::default();
    let bridges: Vec<&dyn HostBridge> = vec![&SshBridge, &fake];
    assert_eq!(bridges.len(), 2);
}
