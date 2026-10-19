//! SWE-bench Verified 适配：sweb.eval 容器内跑 agent，官方 eval_script 判分。
//!
//! 判分链：agent 在 /testbed 改代码 → git diff 取证 → 容器内执行官方 eval_script
//! （内嵌 test_patch 与 START/END 测试段）→ 本地 swev-judge.py（swebench 4.1 官方解析器）裁决。

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
    #[serde(default)]
    pub hints_text: String,
}

impl SwevTask {
    pub fn prompt(&self) -> String {
        let mut text = format!(
            "Repository: {}\n\nIssue description:\n{}",
            self.repo, self.problem_statement
        );
        let hints = self.hints_text.trim();
        if !hints.is_empty() {
            text.push_str("\n\nHints:\n");
            text.push_str(hints);
        }
        text
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
    let script = format!(
        "docker run -d --name {container} \
         -e HTTPS_PROXY=http://172.17.0.1:8123 \
         -e HTTP_PROXY=http://172.17.0.1:8123 \
         -e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
         {} sleep infinity",
        task.image
    );
    match tb_ssh(host, &script, 120) {
        Ok((true, _)) => Ok(tb::TbEnv {
            container,
            task: task.instance_id.clone(),
            host: host.into(),
        }),
        Ok((false, log)) => Err(format!(
            "起容器失败: {}",
            log.chars().take(400).collect::<String>()
        )),
        Err(err) => Err(err),
    }
}

/// 判分：取证 diff → 写入 eval 脚本 → 容器内执行 → 本地官方判分。
/// `task_json_path` 是 swev-prep 产出的原始任务包（判分器全字段来源）。
/// 返回 (resolved, note)。
pub fn verify(
    host: &str,
    env: &tb::TbEnv,
    task: &SwevTask,
    work_dir: &Path,
    task_json_path: &Path,
) -> (Option<bool>, String) {
    let diff_cmd = format!(
        "docker exec {} bash -lc 'cd /testbed && git -c core.fileMode=false diff' ",
        env.container
    );
    let agent_diff = match tb_ssh(host, &diff_cmd, 60) {
        Ok((_, text)) => text,
        Err(err) => return (None, format!("取 diff 失败: {err}")),
    };
    let _ = std::fs::create_dir_all(work_dir);
    let diff_path = work_dir.join("agent.diff");
    if std::fs::write(&diff_path, &agent_diff).is_err() {
        return (None, "写 agent.diff 失败".into());
    }
    let eval_path = work_dir.join("eval.sh");
    if std::fs::write(&eval_path, &task.eval_script).is_err() {
        return (None, "写 eval.sh 失败".into());
    }
    // eval 脚本经 stdin 喂给容器 bash，避免 docker cp 往返。
    let run_cmd = format!("docker exec -i {} bash -s", env.container);
    let log_path = work_dir.join("eval.log");
    let status = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(host)
        .arg(format!("timeout 1200 {run_cmd}"))
        .stdin(std::process::Stdio::from(
            std::fs::File::open(&eval_path).expect("eval.sh 刚写过"),
        ))
        .stdout(std::process::Stdio::from(
            std::fs::File::create(&log_path).expect("eval.log 可建"),
        ))
        .stderr(std::process::Stdio::null())
        .status();
    if status.is_err() {
        return (None, "eval 执行桥接失败".into());
    }
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
                    (resolved, format!("judge: {}", text.trim()))
                }
                Err(err) => (
                    None,
                    format!(
                        "judge 输出不是 JSON: {err}: {}",
                        &text[..text.len().min(200)]
                    ),
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
        ),
        Err(err) => (None, format!("judge 启动失败: {err}")),
    }
}

fn tb_ssh(host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String> {
    let output = Command::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(host)
        .arg(format!(
            "timeout {timeout_sec} bash -c '{}'",
            script.replace('\'', "'\\''")
        ))
        .output()
        .map_err(|err| format!("ssh 启动失败: {err}"))?;
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
