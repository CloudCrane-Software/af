//! SWE-bench Verified 适配：sweb.eval 容器内跑 agent，官方 eval_script 判分。
//!
//! 判分链：agent 在 /testbed 改代码 → git diff 取证 → 容器内执行官方 eval_script
//! （内嵌 test_patch 与 START/END 测试段）→ 本地 swev-judge.py（swebench 4.1 官方解析器）裁决。
//!
//! D-20 证据链加固（2026-10-20，可靠性修复，不改判分语义）：
//! - eval 桥接 timeout 击杀时在 eval.log 末尾写显式标记行 `EVAL_TIMEOUT_KILLED <秒数>`；
//! - 判分时计算 eval.log sha256，随结果行落 eval_sha256 字段（判分复核可逐字节对账）。

use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use crate::tb;

#[derive(Debug, Deserialize)]
pub struct SwevTask {
    pub instance_id: String,
    pub repo: String,
    pub problem_statement: String,
    pub image: String,
    pub eval_script: String,
    /// fix2 对齐后不再注入提示（审计 D4），字段保留只为数据包反序列化完整。
    #[serde(default)]
    #[allow(dead_code)]
    pub hints_text: String,
}

impl SwevTask {
    /// fix2 对齐（设计/harness-对齐审计.md D4/§6.A.5）：不再注入 hints_text——
    /// 三个标准 harness 都只给 problem_statement，官方 hints 是面向解题者的补丁提示，
    /// 注入属"反向虚高"（与榜单不可比）。字段保留（判分器全字段来源不动）。
    pub fn prompt(&self) -> String {
        format!(
            "Repository: {}\n\nIssue description:\n{}",
            self.repo, self.problem_statement
        )
    }
}

pub fn load_swev_tasks(root: &Path, names: &[String]) -> Result<Vec<SwevTask>, String> {
    let mut out = Vec::new();
    for name in names {
        let path = root.join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("读 {} 失败: {err}", path.display()))?;
        let task: SwevTask = serde_json::from_str(&text)
            .map_err(|err| format!("{} 不是 JSON: {err}", path.display()))?;
        out.push(task);
    }
    Ok(out)
}

/// 起容器。sweb.eval 镜像自带 /testbed 与全部测试资产，无需 build。
pub fn setup(host: &str, task: &SwevTask, profile: &str) -> Result<tb::TbEnv, String> {
    let container = format!(
        "afbench-swev-{}-{profile}",
        task.instance_id.replace("__", "-")
    );
    let _ = tb_ssh(host, &format!("docker rm -f {container}"), 30);
    // 出网代理按宿主区分：anolis 容器经宿主 squid（172.17.0.1:8123）；
    // 本机（local）容器走 docker NAT 直连（本机无 8123 监听），语义同为"容器可出网"。
    let proxy_env = if host == "local" {
        ""
    } else {
        "-e HTTPS_PROXY=http://172.17.0.1:8123 \
         -e HTTP_PROXY=http://172.17.0.1:8123 \
         -e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10"
    };
    // fix2 环境卫生（mini-swe-agent env 块同源）：分页器挂死与进度条/着色噪音
    // 是 SWE 容器高频暗坑，容器级环境变量一次性根治。测量语义变更，计入 fix2。
    // 资源护栏（2026-10-21，常量与校准注释见 tb 模块）：agent 容器 6g/512。
    // 基建修复不改判分语义：上限远高于实测峰值（agent 期 30-90MiB）；
    // 本代际判分在该容器内原地执行，判分负载同受此封顶。
    let mem = tb::SWEV_AGENT_CONTAINER_MEM;
    let pids = tb::SWEV_AGENT_CONTAINER_PIDS;
    let script = format!(
        "docker run -d --name {container} \
         --memory {mem} --pids-limit {pids} \
         {proxy_env} \
         -e PAGER=cat -e MANPAGER=cat -e LESS=-R \
         -e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1 \
         {} sleep infinity",
        task.image
    );
    match tb_ssh(host, &script, 120) {
        Ok((true, _)) => Ok(tb::TbEnv {
            container,
            task: task.instance_id.clone(),
            host: host.into(),
            workdir: "/testbed".into(),
        }),
        Ok((false, log)) => Err(format!(
            "起容器失败: {}",
            log.chars().take(400).collect::<String>()
        )),
        Err(err) => Err(err),
    }
}

/// 取证 agent diff（完成度门禁与判分共用）：写 work_dir/agent.diff 并返回文本。
pub fn capture_diff(host: &str, env: &tb::TbEnv, work_dir: &Path) -> Result<String, String> {
    let diff_cmd = format!(
        "docker exec {} bash -lc 'cd /testbed && git -c core.fileMode=false diff' ",
        env.container
    );
    let agent_diff = match tb_ssh(host, &diff_cmd, 60) {
        Ok((_, text)) => text,
        Err(err) => return Err(format!("取 diff 失败: {err}")),
    };
    let _ = std::fs::create_dir_all(work_dir);
    let diff_path = work_dir.join("agent.diff");
    if std::fs::write(&diff_path, &agent_diff).is_err() {
        return Err("写 agent.diff 失败".into());
    }
    Ok(agent_diff)
}

/// diff 是否碰了测试文件（评分 harness 拥有测试，模型改测试=越权，S0 实证缺陷）。
/// 返回被碰的文件列表（空=干净）。
pub fn diff_touches_tests(diff: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(path) = rest.split_whitespace().nth(1) {
                let p = path.trim_start_matches("b/").to_lowercase();
                let is_test = p.contains("test")
                    || p.ends_with("conftest.py")
                    || p.ends_with("tox.ini")
                    || p.ends_with("pytest.ini")
                    || p.ends_with("setup.cfg");
                if is_test {
                    hits.push(path.trim_start_matches("b/").to_string());
                }
            }
        }
    }
    hits
}

/// 判分：取证 diff → 写入 eval 脚本 → 容器内执行 → 本地官方判分。
/// `task_json_path` 是 swev-prep 产出的原始任务包（判分器全字段来源）。
/// 返回 (resolved, note, eval_sha256)——eval_sha256 是判分日志的字节指纹
/// （D-20 项 2：判分可追溯；桥接/判分早期失败时为 None）。
pub fn verify(
    host: &str,
    env: &tb::TbEnv,
    task: &SwevTask,
    work_dir: &Path,
    task_json_path: &Path,
) -> (Option<bool>, String, Option<String>) {
    let _agent_diff = match capture_diff(host, env, work_dir) {
        Ok(d) => d,
        Err(err) => return (None, err, None),
    };
    let diff_path = work_dir.join("agent.diff");
    let eval_path = work_dir.join("eval.sh");
    if std::fs::write(&eval_path, &task.eval_script).is_err() {
        return (None, "写 eval.sh 失败".into(), None);
    }
    // eval 脚本经 stdin 喂给容器 bash，避免 docker cp 往返。
    // stderr 必须并入日志：set -x 轨迹与 START/END 标记走 stderr，官方判分需要。
    // host == "local" 时免 ssh 一跳，其余语义一致。
    // fix2 对齐（审计 D9/§6.A.8）：eval 超时 1200 → 1800s（官方 run_evaluation --timeout 默认）。
    const EVAL_TIMEOUT_SEC: u32 = 1800;
    let run_cmd = format!("docker exec -i {} bash -s 2>&1", env.container);
    let log_path = work_dir.join("eval.log");
    let bridge = format!("timeout {EVAL_TIMEOUT_SEC} {run_cmd}");
    let mut bridge_cmd = if host == "local" {
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&bridge);
        cmd
    } else {
        let mut cmd = Command::new("ssh");
        cmd.arg("-o").arg("BatchMode=yes").arg(host).arg(&bridge);
        cmd
    };
    let status = bridge_cmd
        .stdin(std::process::Stdio::from(
            std::fs::File::open(&eval_path).expect("eval.sh 刚写过"),
        ))
        .stdout(std::process::Stdio::from(
            std::fs::File::create(&log_path).expect("eval.log 可建"),
        ))
        .stderr(std::process::Stdio::null())
        .status();
    let mut eval_timed_out = false;
    match status {
        Err(err) => return (None, format!("eval 执行桥接失败: {err}"), None),
        Ok(st) => {
            if st.code() == Some(124) {
                // D-20 项 4：timeout 击杀显式化——在日志末尾落标记行，
                // 不再留无标记截断日志（判分复核据此与"截断/失败日志"灰区区分）。
                eval_timed_out = true;
                let marker = format!("\nEVAL_TIMEOUT_KILLED {EVAL_TIMEOUT_SEC}\n");
                if let Ok(mut log) = std::fs::OpenOptions::new().append(true).open(&log_path) {
                    use std::io::Write as _;
                    let _ = log.write_all(marker.as_bytes());
                }
            }
        }
    }
    // D-20 项 2：判分时计算日志指纹（含超时标记行——哈希的是判分器实际读到的字节）。
    let eval_sha256 = sha256_hex(&log_path);
    let judge = Command::new("/home/pan-ding/swev-venv/bin/python")
        .arg("/home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py")
        .arg(task_json_path)
        .arg(&log_path)
        .arg(&diff_path)
        .output();
    match judge {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<serde_json::Value>(text.trim()) {
                Ok(row) => {
                    let resolved = row.get("resolved").and_then(serde_json::Value::as_bool);
                    // D-20 项 3 消费端：judge JSON 带 log_parsed 时在 note 前缀如实透传
                    // （截断日志的判 false 与"真失败"不再同貌）。
                    let parsed_note = match row.get("log_parsed") {
                        Some(serde_json::Value::Bool(false)) => "[log_parsed=false] ".to_string(),
                        _ => String::new(),
                    };
                    let timeout_note = if eval_timed_out {
                        format!("eval 超时击杀（已落标记 {EVAL_TIMEOUT_SEC}s）；")
                    } else {
                        String::new()
                    };
                    (
                        resolved,
                        format!("{timeout_note}{parsed_note}judge: {}", text.trim()),
                        eval_sha256,
                    )
                }
                Err(err) => (
                    None,
                    format!(
                        "judge 输出不是 JSON: {err}: {}",
                        &text[..text.len().min(200)]
                    ),
                    eval_sha256,
                ),
            }
        }
        Ok(output) => (
            None,
            format!(
                "judge 退出 {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(300)
                    .collect::<String>()
            ),
            eval_sha256,
        ),
        Err(err) => (None, format!("judge 启动失败: {err}"), eval_sha256),
    }
}

/// eval.log 的 sha256 十六进制串；读不到文件时 None（D-20 项 2）。
fn sha256_hex(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(path).ok()?;
    let digest = Sha256::digest(&data);
    Some(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn tb_ssh(host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String> {
    let quoted = format!(
        "timeout {timeout_sec} bash -c '{}'",
        script.replace('\'', "'\\''")
    );
    let output = if host == "local" {
        Command::new("bash").arg("-c").arg(&quoted).output()
    } else {
        Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(host)
            .arg(&quoted)
            .output()
    }
    .map_err(|err| format!("命令桥接启动失败: {err}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    Ok((output.status.success(), text))
}
