//! Terminal-Bench 4.0 适配：任务容器在 --host 指定的 docker 宿主上跑
//! （anolis=经 ssh 桥接，local=本机直执行），bash 工具统一经 docker exec 桥接。
//!
//! 生命周期：rsync 任务目录 → build 环境镜像 → 起容器 → harness Run（bash 工具）→
//! 提取 /app → build 验证器镜像 → 挂载跑 test.sh → 读 reward.txt → 清容器。
//! 判分口径：reward.txt=1 为 pass；验证器缺 reward 记 infra（不占分母，50-方案口径）。

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

pub struct TbTask {
    pub task_id: String,
    pub prompt: String,
}

pub fn load_tb_tasks(root: &Path, names: &[String]) -> Result<Vec<TbTask>, String> {
    let mut out = Vec::new();
    for name in names {
        let dir = root.join(name);
        let prompt_path = dir.join("instruction.md");
        let prompt = std::fs::read_to_string(&prompt_path)
            .map_err(|err| format!("读 {} 失败: {err}", prompt_path.display()))?;
        if !dir.join("environment").join("Dockerfile").is_file() {
            return Err(format!("{name} 缺 environment/Dockerfile"));
        }
        if !dir.join("tests").join("test.sh").is_file() {
            return Err(format!("{name} 缺 tests/test.sh"));
        }
        out.push(TbTask {
            task_id: name.clone(),
            prompt,
        });
    }
    Ok(out)
}

/// 执行一条宿主命令，返回 (status, stdout+stderr 合并文本)。
/// host == "local" 时本机直接执行（本机 docker 直执行面，免 ssh 一跳，
/// 2026-10-20 新增，测量语义不变：只是换了 docker 宿主）；其余经 ssh <host>。
/// 两种路径都用 timeout 包裹，超时语义一致。
fn ssh(host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String> {
    let quoted = format!("timeout {timeout_sec} bash -c {}", shell_quote(script));
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

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub struct TbEnv {
    pub container: String,
    pub task: String,
    pub host: String,
    pub workdir: String,
}

impl TbEnv {
    /// 建环境与容器。任务目录须已在 anolis:/data/bench/tb/<task>。
    pub fn setup(host: &str, task: &TbTask, profile: &str) -> Result<Self, String> {
        let task = &task.task_id;
        let image = format!("afbench-tbenv-{task}");
        let container = format!("afbench-{task}-{profile}");
        let remote_dir = format!("/data/bench/tb/{task}");
        let (ok, log) = ssh(
            host,
            &format!(
                "docker build -q --network host \
                 --build-arg HTTPS_PROXY=http://127.0.0.1:8123 \
                 --build-arg HTTP_PROXY=http://127.0.0.1:8123 \
                 --build-arg NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
                  \
                  \
                 -t {image} {remote_dir}/environment"
            ),
            1800,
        )?;
        if !ok {
            return Err(format!("build {image} 失败: {}", tail(&log, 800)));
        }
        let _ = ssh(host, &format!("docker rm -f {container}"), 30);
        let (ok, log) = ssh(
            host,
            &format!(
                "docker run -d --name {container} \
                 -e HTTPS_PROXY=http://172.17.0.1:8123 \
                 -e HTTP_PROXY=http://172.17.0.1:8123 \
                 -e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
                 {image} sleep infinity"
            ),
            60,
        )?;
        if !ok {
            return Err(format!("起容器 {container} 失败: {}", tail(&log, 400)));
        }
        Ok(Self {
            container,
            task: task.clone(),
            host: host.into(),
            workdir: "/app".into(),
        })
    }

    /// 任务工作目录（TB=/app，SWE-V=/testbed）——bash 工具每条命令都先 cd 到这里。
    pub fn workdir(&self) -> &str {
        &self.workdir
    }

    /// bash 工具体：容器内执行，截断返回。
    pub fn exec(&self, command: &str) -> String {
        let script = format!(
            "docker exec {} bash -lc {}",
            self.container,
            shell_quote(&format!("cd {} && {}", self.workdir, command))
        );
        match ssh(&self.host, &script, 180) {
            Ok((ok, mut text)) => {
                const CAP: usize = 12_000;
                if text.len() > CAP {
                    let keep = CAP / 2;
                    text = format!(
                        "{}\\n...[截断 {} 字节]...\\n{}",
                        &text[..keep],
                        text.len() - keep * 2,
                        &text[text.len() - keep..]
                    );
                }
                format!("exit={}\\n{text}", if ok { 0 } else { 1 })
            }
            Err(err) => format!("exit=127\\nexec 桥接失败: {err}"),
        }
    }

    /// 判分：/app 提取 → 验证器镜像 → 挂载跑 test.sh → reward.txt。
    /// 返回 (reward, verifier_log_tail)。reward: Some(true)=pass Some(false)=fail None=infra。
    pub fn verify(&self) -> (Option<bool>, String) {
        let vol = format!("/data/bench/vol/{}-{}", self.task, self.container);
        // 旧卷里有 root 所有的验证器产物，pan-ding 删不动——用容器内的 root 清。
        let _ = ssh(
            &self.host,
            &format!(
                "docker run --rm -v {vol}:/vol --entrypoint sh python:3.12-slim -c 'rm -rf /vol/* /vol/.[!.]*; chmod 777 /vol' 2>/dev/null || rm -rf {vol}"
            ),
            120,
        );
        match ssh(
            &self.host,
            &format!(
                "mkdir -p {vol}/app {vol}/logs && docker cp {}:/app/. {vol}/app/",
                self.container
            ),
            300,
        ) {
            Ok((true, _)) => {}
            Ok((false, log)) => return (None, format!("提取 /app 失败: {}", tail(&log, 300))),
            Err(err) => return (None, format!("提取 /app 失败: {err}")),
        }
        let verifier = format!("afbench-tbver-{}", self.task);
        let remote_dir = format!("/data/bench/tb/{}", self.task);
        match ssh(
            &self.host,
            &format!(
                "docker build -q --network host \
                 --build-arg HTTPS_PROXY=http://127.0.0.1:8123 \
                 --build-arg HTTP_PROXY=http://127.0.0.1:8123 \
                 --build-arg NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
                  \
                  \
                 -t {verifier} {remote_dir}/tests"
            ),
            1800,
        ) {
            Ok((true, _)) => {}
            Ok((false, log)) => return (None, format!("build 验证器失败: {}", tail(&log, 600))),
            Err(err) => return (None, format!("build 验证器失败: {err}")),
        }
        let run_log = match ssh(
            &self.host,
            &format!(
                "docker run --rm -v {vol}/app:/app -v {vol}/logs:/logs {verifier} bash /tests/test.sh"
            ),
            1200,
        ) {
            Ok((_, text)) => text,
            Err(err) => return (None, format!("验证器执行失败: {err}")),
        };
        let reward_cmd = format!("cat {vol}/logs/verifier/reward.txt 2>/dev/null || echo missing");
        match ssh(&self.host, &reward_cmd, 30) {
            Ok((_, text)) => {
                let reward = text.trim();
                match reward {
                    "1" => (Some(true), tail(&run_log, 600)),
                    "0" => (Some(false), tail(&run_log, 600)),
                    _ => (
                        None,
                        format!("无 reward.txt；验证器输出: {}", tail(&run_log, 400)),
                    ),
                }
            }
            Err(err) => (None, format!("读 reward 失败: {err}")),
        }
    }

    pub fn cleanup(&self) {
        let _ = ssh(&self.host, &format!("docker rm -f {}", self.container), 60);
    }
}

fn tail(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        text.to_string()
    } else {
        text[text.len() - cap..].to_string()
    }
}

/// bash 工具声明（ToolDecl JSON）。`workdir` 写进描述，与 exec 的 cd 前缀一致，
/// 避免"工具描述说 /app、实际在 /testbed"的目录矛盾（S0 零补丁缺陷嫌疑之一）。
pub fn bash_tool_decl(workdir: &str) -> Value {
    json!({
        "name": "bash",
        "description": format!("Execute a bash command in the task environment (a Linux container). Every command runs with the working directory set to {workdir} (equivalent to a leading `cd {workdir}`). Returns exit code, stdout and stderr. Use it to inspect, modify, build and test."),
        "parameters": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The bash command to execute."}
            },
            "required": ["command"]
        },
        "effect": "external_once"
    })
}
