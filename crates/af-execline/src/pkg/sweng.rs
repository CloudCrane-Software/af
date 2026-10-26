//! 软件工程配置包 v1 真实现——15 件组件（G4 设计《G4-软件工程包设计-v1》
//! §2.2 tool 11 件 / §2.3 unit 4 件，逐件对应）。
//!
//! 宿主与绑定（§2.1 总则）：进程内注册表，住执行面包内新模块——语言核心零
//! 改动。出边界两条既有缝：
//! - **HostBridge 容器 exec**（工具类）：`TbEnv` 句柄（container/task/host/
//!   workdir）+ [`crate::suite::tb`] 的 `shell_quote`/`split_rc` 先例——
//!   [`container_exec`] 为 `TbEnv::exec`（tb.rs:222-258）的超时参数化镜像，
//!   输出契约逐条一致（exit 尾行/超时教学/12k 截断）。
//! - **swev 判分链复用**（U1）：[`crate::suite::swev::verify` 全链 + judge.rs
//!   三原则]——指纹门→取证 diff→eval 容器执行→本机 judge 子进程；本模块零
//!   判分逻辑复刻。
//!
//! impl_ref 锚约定（裁决13 双形态入契约）：真实现件统一
//! `executor:builtin/<组件名>@af-execline`（无版本段——版本进 crate 名前的
//! 组件名面）；与包路径形态 `packages/<pkg>@<ver>/...` 双合法，装订器按前缀
//! 判别（[`builtin_target`]）。声明面（components.json 四字段）不动 name/
//! version/class/effect_class——只改写 impl_ref 一个槽。
//!
//! ToolDecl 双轨（§2.1）：声明面/模型面分离；模型面表 = [`tool_decl`]，
//! T1 复用 `tb::bash_tool_decl` 先例（name="bash"），其余 10 件以组件名段为
//! 模型面名（去 `pkg.` 与 `@版本`，跨包互异）。
//!
//! 真实现分级（裁决17）：E2E 路径件 = T1（env/bash，`EnvBash`）+ U1
//! （swebench_eval，全链）；其余 13 件单元级实测（FakeBridge 断言 + 真容器
//! 抽样 ≥1 例/件，tests/pkg_sweng.rs）。真实模型执行的端到端=铺单批
//! F-第四单，不在本批。

use std::path::Path;
use std::sync::{Arc, Mutex};

use af_harness::ToolDecl;
use serde_json::{json, Value};

use crate::error::ExeclineError;
use crate::hostbridge::HostBridge;
use crate::judge::JudgeLocator;
use crate::suite::swev::{self, SwevTask};
use crate::suite::swev_unit::bash_guard_wrap;
use crate::suite::tb::{self, shell_quote, split_rc, TbEnv};

// ============================================================
// impl_ref 锚约定（裁决13）
// ============================================================

/// builtin 锚前缀（与包路径形态 `packages/<pkg>@<ver>/...` 双合法的分界）。
pub const BUILTIN_IMPL_PREFIX: &str = "executor:builtin/";
/// builtin 锚的宿主 crate 段。
pub const BUILTIN_HOST_CRATE: &str = "af-execline";

/// 15 件组件的注册面引用串（name@version，与 af/packages 声明逐字一致）。
pub const REF_ENV: &str = "pkg.sweagent.env@1";
pub const REF_ACI_SEARCH: &str = "pkg.sweagent.aci.search@1";
pub const REF_ACI_OPEN: &str = "pkg.sweagent.aci.open@1";
pub const REF_ACI_EDIT: &str = "pkg.sweagent.aci.edit@1";
pub const REF_ACI_CREATE: &str = "pkg.sweagent.aci.create@1";
pub const REF_ACI_RUN_TEST: &str = "pkg.sweagent.aci.run_test@1";
pub const REF_ACI_SUBMIT: &str = "pkg.sweagent.aci.submit@1";
pub const REF_SWENG_LOCATE: &str = "pkg.sweng.locate@1";
pub const REF_SWENG_VIEW: &str = "pkg.sweng.view@1";
pub const REF_SWENG_EDIT: &str = "pkg.sweng.edit@1";
pub const REF_SWENG_PUBLISH: &str = "pkg.sweng.publish_task_finished@1";
pub const REF_SWEBENCH_EVAL: &str = "pkg.sweagent.swebench_eval@1";
pub const REF_ACI_SCHEMA: &str = "pkg.sweagent.aci_schema@1";
pub const REF_SWENG_TEST_UNIT: &str = "pkg.sweng.test_unit@1";
pub const REF_HUMAN_GATE: &str = "pkg.sweagent.human_gate@1";

/// 15 件全集（验收/对账面；tests/pkg_sweng.rs 锁 15 计数）。
pub const SWENG_V1_COMPONENTS: &[&str] = &[
    REF_ENV,
    REF_ACI_SEARCH,
    REF_ACI_OPEN,
    REF_ACI_EDIT,
    REF_ACI_CREATE,
    REF_ACI_RUN_TEST,
    REF_ACI_SUBMIT,
    REF_SWENG_LOCATE,
    REF_SWENG_VIEW,
    REF_SWENG_EDIT,
    REF_SWENG_PUBLISH,
    REF_SWEBENCH_EVAL,
    REF_ACI_SCHEMA,
    REF_SWENG_TEST_UNIT,
    REF_HUMAN_GATE,
];

/// 引用串取名段（`pkg.x.y@1` → `pkg.x.y`；无版本段原样）。
pub fn name_of(reference: &str) -> &str {
    reference
        .split_once('@')
        .map_or(reference, |(name, _)| name)
}

/// 组 builtin 锚：`executor:builtin/<名>@af-execline`（名不带版本段）。
pub fn builtin_impl_ref(name: &str) -> String {
    format!("{BUILTIN_IMPL_PREFIX}{name}@{BUILTIN_HOST_CRATE}")
}

/// 是否 builtin 锚形态（装订器前缀判别——裁决13）。
pub fn is_builtin_impl(impl_ref: &str) -> bool {
    builtin_target(impl_ref).is_some()
}

/// 拆 builtin 锚 → `(组件名, 宿主 crate)`；非本形态返回 None。
pub fn builtin_target(impl_ref: &str) -> Option<(String, String)> {
    let rest = impl_ref.strip_prefix(BUILTIN_IMPL_PREFIX)?;
    let (name, host) = rest.rsplit_once('@')?;
    if name.is_empty() || host.is_empty() || host.contains('/') {
        return None;
    }
    Some((name.to_string(), host.to_string()))
}

// ============================================================
// ToolDecl 表（模型面声明；§2.1 双轨）
// ============================================================

fn decl_from_json(name: &str, description: &str, parameters: Value, effect: &str) -> ToolDecl {
    serde_json::from_value(json!({
        "name": name,
        "description": description,
        "parameters": parameters,
        "effect": effect,
    }))
    .expect("ToolDecl 静态表构造必须合法（af-harness spec.rs 契约）")
}

/// 模型面名：组件名去 `pkg.` 前缀（跨包互异；T1 例外——bash 先例 name=bash）。
fn model_name(reference: &str) -> String {
    name_of(reference)
        .strip_prefix("pkg.")
        .unwrap_or_else(|| name_of(reference))
        .to_string()
}

/// ToolDecl 表查找（T1 按运行时 workdir 生成 bash 先例声明；unit 类无模型面）。
pub fn tool_decl(reference: &str, workdir: &str) -> Option<ToolDecl> {
    match name_of(reference) {
        "pkg.sweagent.env" => serde_json::from_value(tb::bash_tool_decl(workdir)).ok(),
        "pkg.sweagent.aci.search" => Some(decl_from_json(
            &model_name(reference),
            "Read-only code search in the repository container: regex pattern over file contents, returns matching lines as path:line:text, truncated to max_hits.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Extended regular expression to search for."},
                    "path": {"type": "string", "description": "File or directory to search (default: the repository workdir)."},
                    "max_hits": {"type": "integer", "description": "Maximum number of matching lines to return (default 50)."}
                },
                "required": ["pattern"]
            }),
            "pure",
        )),
        "pkg.sweagent.aci.open" | "pkg.sweng.view" => Some(decl_from_json(
            &model_name(reference),
            "Read a line range of a file in the repository container, returned as lineno<TAB>text.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read."},
                    "start_line": {"type": "integer", "description": "First line to return (default 1)."},
                    "end_line": {"type": "integer", "description": "Last line to return (default start_line+99)."}
                },
                "required": ["path"]
            }),
            "pure",
        )),
        "pkg.sweagent.aci.edit" | "pkg.sweng.edit" => Some(decl_from_json(
            &model_name(reference),
            "Soft-aligned edit: old_text must occur exactly once in the file; it is replaced with new_text. Zero or multiple matches are reported as an error observation (retry with more context).",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old_text": {"type": "string", "description": "Existing text to replace (must match exactly once)."},
                    "new_text": {"type": "string", "description": "Replacement text."}
                },
                "required": ["path", "old_text", "new_text"]
            }),
            "persist",
        )),
        "pkg.sweagent.aci.create" => Some(decl_from_json(
            &model_name(reference),
            "Create a new file with the given content. The parent directory must already exist; an existing path is an error.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string", "description": "Full file content to write."}
                },
                "required": ["path", "content"]
            }),
            "persist",
        )),
        "pkg.sweagent.aci.run_test" => Some(decl_from_json(
            &model_name(reference),
            "Run the repository test command in the container (default: the environment's declared test command). Output is merged stdout+stderr followed by a final exit=<code> line.",
            json!({
                "type": "object",
                "properties": {
                    "test_cmd": {"type": "string", "description": "Test command override (default: env_handle.test_command)."},
                    "timeout_sec": {"type": "integer", "description": "Kill timeout in seconds (default 300)."}
                }
            }),
            "external_once",
        )),
        "pkg.sweagent.aci.submit" => Some(decl_from_json(
            &model_name(reference),
            "Submit the task: prints the TASK_COMPLETE sentinel as the first output line and finalizes the forensic diff (agent.diff) as the workspace artifact.",
            json!({
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "One-paragraph summary of the fix."}
                }
            }),
            "persist",
        )),
        "pkg.sweng.locate" => Some(decl_from_json(
            &model_name(reference),
            "Locate candidate code positions: read-only regex search returning path:line:text candidates (MetaGPT engineer face).",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Regex to locate."},
                    "path": {"type": "string", "description": "Scope (default: repository workdir)."},
                    "max_hits": {"type": "integer", "description": "Truncation bound (default 50)."}
                },
                "required": ["query"]
            }),
            "pure",
        )),
        "pkg.sweng.publish_task_finished" => Some(decl_from_json(
            &model_name(reference),
            "Publish a node_event(done) row plus the work-artifact registration (idempotent per node).",
            json!({
                "type": "object",
                "properties": {
                    "summary": {"type": "string", "description": "Completion summary."},
                    "patch_ref": {"type": "string", "description": "Reference to the finished patch artifact (e.g. agent.diff)."}
                },
                "required": ["summary"]
            }),
            "persist",
        )),
        _ => None,
    }
}

// ============================================================
// 容器执行载体（TbEnv::exec 的超时参数化镜像；tb.rs:222-258 契约一致）
// ============================================================

/// 观察文本剥离末行 `exit=<n>`：`(正文, 退出码)`——无标记行时 `(全文, None)`
/// （外层 timeout 击杀面）。工具侧后处理（T3 空读判定 / U3 退出码判定）用。
pub(crate) fn split_observation(text: &str) -> (String, Option<i32>) {
    if let Some(rc) = text.strip_prefix("exit=") {
        if let Ok(code) = rc.trim().parse() {
            return (String::new(), Some(code));
        }
    }
    if let Some(pos) = text.rfind("\nexit=") {
        if let Ok(code) = text[pos + 6..].trim().parse() {
            return (text[..pos].to_string(), Some(code));
        }
    }
    (text.to_string(), None)
}

/// 容器内执行一条命令，返回完整工具观察文本（输出契约 = `TbEnv::exec`
/// 逐条一致：`__AF_RC` 标记剥离、超时教学反馈、12k 双端截断、`exit=<n>` 尾行；
/// 差异仅一点——桥接 timeout 由调用方给定，不钉 180）。
pub fn container_exec(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    command: &str,
    timeout_sec: u32,
) -> String {
    let script = format!(
        "docker exec {} bash -lc {}; echo __AF_RC=$?",
        env.container,
        shell_quote(&format!("cd {} && {}", env.workdir, command))
    );
    let (mut text, rc) = match bridge.run(&env.host, &script, timeout_sec) {
        Ok((_, raw)) => split_rc(&raw),
        Err(err) => {
            return format!(
                "[harness] The execution bridge failed before the command could produce output (transient infrastructure error: {err}). This is not a task failure — retry the command.\nexit=127"
            );
        }
    };
    if rc.is_none() {
        text.push_str(&format!(
            "\n[harness] The command was killed after exceeding the {timeout_sec}s timeout (no exit code was reported). Partial output (if any) is above. Re-run with a different approach: pipe input or use non-interactive flags (-y / -n / DEBIAN_FRONTEND=noninteractive), wrap long work in an explicit `timeout <secs> ...`, and avoid commands that never exit on their own (servers, watch loops, pagers)."
        ));
    }
    const CAP: usize = 12_000;
    if text.len() > CAP {
        let keep = CAP / 2;
        text = format!(
            "{}\n...[output truncated: {} bytes elided. Re-run with a narrower command: pipe through head/tail/sed -n, grep for specific patterns, or redirect output to a file and read it in parts.]\n{}",
            &text[..keep],
            text.len() - keep * 2,
            &text[text.len() - keep..]
        );
    }
    text.push_str(&format!(
        "\nexit={}",
        rc.map(|n| n.to_string())
            .unwrap_or_else(|| "124".to_string())
    ));
    text
}

// ============================================================
// T1 · pkg.sweagent.env（E2E 主链真实现：bash_tool 先例直接复用）
// ============================================================

/// T1：容器 bash 面——`TbEnv` 执行 + 反考古注入 + 重复命令 ≥3 拦截
/// （swev_unit.rs `bash_tool` 闭包语义原样，护栏复用 [`bash_guard_wrap`]）。
/// 幂等键语义（external_once）：同 `{stable_id}/{node}/{unit}/{attempt}` 重放
/// 由执行面 effect 记账面承接，本工具不自建计数。
pub struct EnvBash {
    bridge: Arc<dyn HostBridge + Send + Sync>,
    env: Arc<TbEnv>,
    recent: Mutex<Vec<String>>,
}

impl EnvBash {
    /// 构造（Arc 形态：工具闭包整体 'static + Send + Sync，af-harness 注册面）。
    pub fn new(bridge: Arc<dyn HostBridge + Send + Sync>, env: Arc<TbEnv>) -> Self {
        Self {
            bridge,
            env,
            recent: Mutex::new(Vec::new()),
        }
    }

    /// 执行一条 bash 命令（timeout_sec 缺省 60，§2.2-T1）。
    pub fn call(&self, command: &str, timeout_sec: Option<u32>) -> String {
        let out = container_exec(
            self.bridge.as_ref(),
            &self.env,
            command,
            timeout_sec.unwrap_or(60),
        );
        let mut hist = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        bash_guard_wrap(command, out, &mut hist)
    }
}

// ============================================================
// T2/T8 · 只读检索（pure）
// ============================================================

/// T2 `pkg.sweagent.aci.search`：`grep -rnE -I -- <pattern> <path> | head -n <max>`
/// （命令由类型化参数自构——只读白名单即构造面本身；模式/路径经 shell_quote）。
pub fn aci_search(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    pattern: &str,
    path: Option<&str>,
    max_hits: Option<u32>,
) -> String {
    let target = path.unwrap_or(env.workdir.as_str());
    let max = max_hits.unwrap_or(50);
    container_exec(
        bridge,
        env,
        &format!(
            "grep -rnE -I -- {} {} | head -n {max}",
            shell_quote(pattern),
            shell_quote(target)
        ),
        60,
    )
}

/// T8 `pkg.sweng.locate`：同 T2（query 参数名面，MetaGPT 工程师面）。
pub fn sweng_locate(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    query: &str,
    path: Option<&str>,
    max_hits: Option<u32>,
) -> String {
    aci_search(bridge, env, query, path, max_hits)
}

// ============================================================
// T3/T9 · 只读取片段（pure）
// ============================================================

fn open_lines(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    path: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    let start = start_line.unwrap_or(1);
    let end = end_line.unwrap_or(start + 99);
    if start == 0 || end < start {
        return format!("ERROR: invalid line range {start}..{end} (need start>=1 and end>=start)");
    }
    let prog = format!("NR>={start} && NR<={end} {{ print NR \"\\t\" $0 }}");
    let text = container_exec(
        bridge,
        env,
        &format!("awk {} {}", shell_quote(&prog), shell_quote(path)),
        60,
    );
    let (body, rc) = split_observation(&text);
    if rc == Some(0) && body.trim().is_empty() {
        return format!("ERROR: no lines in range {start}..{end} for {path}");
    }
    text
}

/// T3 `pkg.sweagent.aci.open`：带行号读取文件片段；越界/空读 → 报错观察。
pub fn aci_open(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    path: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    open_lines(bridge, env, path, start_line, end_line)
}

/// T9 `pkg.sweng.view`：同 T3（start/end 参数名面）。
pub fn sweng_view(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    path: &str,
    start: Option<u32>,
    end: Option<u32>,
) -> String {
    open_lines(bridge, env, path, start, end)
}

// ============================================================
// T4/T10 · 软对齐编辑（persist）
// ============================================================

/// 编辑载体脚本（容器内 python3；无单引号——shell_quote 层安全；载荷以
/// JSON argv 直传，旧文/新文原文字节进 argv，无 shell 再解释）。
const EDIT_SCRIPT: &str = "import sys,json
p=json.loads(sys.argv[1])
path=p[\"path\"];old=p[\"old\"];new=p[\"new\"]
try:
    data=open(path,encoding=\"utf-8\").read()
except OSError as e:
    print(\"ERROR read \"+path+\": \"+str(e));sys.exit(1)
n=data.count(old)
if n!=1:
    print(\"ERROR %d matches for old_text in %s (need exactly 1)\"%(n,path));sys.exit(1)
open(path,\"w\",encoding=\"utf-8\").write(data.replace(old,new,1))
print(\"OK \"+path)";

/// T4 `pkg.sweagent.aci.edit`：`old_text` 恰一次出现才替换（0/>1 → 报错观察，
/// 模型重试）；persist——同幂等键重放不重复应用由执行面记账承接。
pub fn aci_edit(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    path: &str,
    old_text: &str,
    new_text: &str,
) -> String {
    let payload = json!({ "path": path, "old": old_text, "new": new_text }).to_string();
    container_exec(
        bridge,
        env,
        &format!(
            "python3 -c {} {}",
            shell_quote(EDIT_SCRIPT),
            shell_quote(&payload)
        ),
        60,
    )
}

/// T10 `pkg.sweng.edit`：同 T4 契约。
pub fn sweng_edit(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    path: &str,
    old_text: &str,
    new_text: &str,
) -> String {
    aci_edit(bridge, env, path, old_text, new_text)
}

// ============================================================
// T5 · 新建文件（persist）
// ============================================================

const CREATE_SCRIPT: &str = "import sys,json,os
p=json.loads(sys.argv[1])
path=p[\"path\"];content=p[\"content\"]
d=os.path.dirname(path) or \".\"
if not os.path.isdir(d):
    print(\"ERROR parent directory missing: \"+d);sys.exit(1)
if os.path.exists(path):
    print(\"ERROR file exists: \"+path);sys.exit(1)
open(path,\"w\",encoding=\"utf-8\").write(content)
print(\"OK \"+path)";

/// T5 `pkg.sweagent.aci.create`：父目录须已在；已存在 → 报错观察。
pub fn aci_create(bridge: &dyn HostBridge, env: &TbEnv, path: &str, content: &str) -> String {
    let payload = json!({ "path": path, "content": content }).to_string();
    container_exec(
        bridge,
        env,
        &format!(
            "python3 -c {} {}",
            shell_quote(CREATE_SCRIPT),
            shell_quote(&payload)
        ),
        60,
    )
}

// ============================================================
// T6 · 测试执行（external_once）
// ============================================================

/// T6 `pkg.sweagent.aci.run_test`：容器内执行测试命令——缺省取
/// `env_handle.test_command`（调用方持有，本函数以 `default_test_command`
/// 收口）；超时击杀显式标记（exit=124 + 教学行，承 swev.rs:274-289 口径）。
pub fn aci_run_test(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    default_test_command: &str,
    test_cmd: Option<&str>,
    timeout_sec: Option<u32>,
) -> String {
    let cmd = test_cmd.unwrap_or(default_test_command);
    container_exec(bridge, env, cmd, timeout_sec.unwrap_or(300))
}

// ============================================================
// T7 · 提交（persist：哨兵 + 取证 diff 定稿）
// ============================================================

/// T7 `pkg.sweagent.aci.submit`：首行恒 `TASK_COMPLETE`（StopCondition
/// completion_signals 命中面，swev_unit.rs:351-356 组装先例）；同时触发
/// 取证 diff（`swev::capture_diff`）定稿为 workspace 工件 `agent.diff`——
/// 取证失败不回撤哨兵（完成信号语义优先），失败文案如实入观察。
pub fn aci_submit(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    work_dir: &Path,
    summary: Option<&str>,
) -> String {
    let diff = swev::capture_diff(bridge, &env.host, env, work_dir);
    let mut out = String::from("TASK_COMPLETE\n");
    if let Some(summary) = summary {
        out.push_str(summary);
        out.push('\n');
    }
    match diff {
        Ok(text) => out.push_str(&format!("agent.diff: {} bytes", text.len())),
        Err(err) => out.push_str(&format!("agent.diff capture failed: {err}")),
    }
    out
}

// ============================================================
// T11 · 发布完成（persist：MES node_event(done) + 工件登记，幂等）
// ============================================================

/// T11 `pkg.sweng.publish_task_finished`：向 `work_dir/node-events.jsonl`
/// 追加 `node_event(done)` 行 + 工件登记字段（`WorkArtifact` 附加面）；
/// 幂等 = 同节点 done 以 event_id `{node}/done` 去重（承 mes-ch-shipped
/// marker 只补差量语义）；返回 `PUBLISHED <artifact_id>`。
pub fn publish_task_finished(
    work_dir: &Path,
    node_id: &str,
    summary: &str,
    patch_ref: Option<&str>,
) -> Result<String, ExeclineError> {
    let artifact_id = format!("{node_id}-finish");
    let event_id = format!("{node_id}/done");
    let path = work_dir.join("node-events.jsonl");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let marker = format!("\"event_id\":\"{event_id}\"");
    if existing.contains(&marker) {
        return Ok(format!("PUBLISHED {artifact_id}"));
    }
    let row = json!({
        "event": "node_event",
        "kind": "done",
        "node": node_id,
        "event_id": event_id,
        "summary": summary,
        "patch_ref": patch_ref,
        "artifact": { "artifact_id": artifact_id, "is_final": true, "qualified": false },
    });
    std::fs::create_dir_all(work_dir).map_err(|err| ExeclineError::LedgerIo {
        detail: format!("建事件目录失败 {}: {err}", work_dir.display()),
    })?;
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| ExeclineError::LedgerIo {
            detail: format!("开 {} 失败: {err}", path.display()),
        })?;
    writeln!(file, "{row}").map_err(|err| ExeclineError::LedgerIo {
        detail: format!("写 {} 失败: {err}", path.display()),
    })?;
    Ok(format!("PUBLISHED {artifact_id}"))
}

// ============================================================
// U1 · pkg.sweagent.swebench_eval（E2E 主链真实现：swev 判分链复用）
// ============================================================

/// U1 产出：judge 透传 verdict + 链路元数据。
#[derive(Debug, Clone)]
pub struct SwebenchVerdict {
    /// resolved ∈ {true,false}=有结论；null=供给面失败（不通过）。
    pub resolved: Option<bool>,
    /// verdict JSON（judge stdout 原样透传 + eval_sha256 注入）。
    pub verdict: Value,
    /// 判分链 note（超时/log_parsed 前缀 + judge 原文）。
    pub note: String,
    /// 判分日志字节指纹（swev.rs:346-351 D-20 项 2）。
    pub eval_sha256: Option<String>,
}

/// 判定装配（纯函数面）：judge stdout JSON 原样透传 + eval_sha256 注入——
/// Rust 侧零再解释（judge.rs 三原则 §3.2-3）；无 judge JSON（供给面失败）→
/// `{resolved: null, error: note}`。
pub fn verdict_from_verify(
    resolved: Option<bool>,
    note: &str,
    eval_sha256: Option<String>,
) -> Value {
    let sha_value = eval_sha256
        .clone()
        .map_or_else(|| Value::Null, |sha| json!(sha));
    if let Some((_, tail)) = note.rsplit_once("judge: ") {
        if let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(tail.trim()) {
            map.insert("eval_sha256".into(), sha_value);
            return Value::Object(map);
        }
    }
    json!({ "resolved": resolved, "error": note, "eval_sha256": sha_value })
}

/// U1：swev 判分三件套链复用（`swev::verify` 全链：指纹门→capture_diff→
/// eval.sh 容器执行 1800s→本机 judge 子进程）。独立性锚：①子进程边界——
/// 本模块零判分逻辑；②argv 恰三位置参数（judge.rs:5-8,77-89）；③判分脚本
/// sha256 锚 `a292547f…` 双门（装载期+verify 期，judge.rs:43-50,56-70）；
/// ④实现面（提示词/循环/工具）改动碰不到判分。
pub fn swebench_eval(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    judge: &JudgeLocator,
    task: &SwevTask,
    work_dir: &Path,
    task_json: &Path,
) -> SwebenchVerdict {
    let (resolved, note, eval_sha256) =
        swev::verify(bridge, judge, &env.host, env, task, work_dir, task_json);
    let verdict = verdict_from_verify(resolved, &note, eval_sha256.clone());
    SwebenchVerdict {
        resolved,
        verdict,
        note,
        eval_sha256,
    }
}

// ============================================================
// U2 · pkg.sweagent.aci_schema（纯结构校验，零 LLM）
// ============================================================

/// U2：`agent_action` 结构校验（SWE-agent.yaml:30-37 面）——字段在位/类型
/// 核对；出参 `format_verdict {error, submit}`（error 空=过）。
/// 静态确定性函数：同输入恒同输出。
pub fn aci_schema(action: &Value) -> Value {
    let Some(map) = action.as_object() else {
        return json!({ "error": "agent_action must be a JSON object", "submit": false });
    };
    let mut errors: Vec<String> = Vec::new();
    for (field, kind, check) in [
        ("thought", "string", Value::is_string as fn(&Value) -> bool),
        ("command", "string", Value::is_string as fn(&Value) -> bool),
        ("submit", "boolean", Value::is_boolean as fn(&Value) -> bool),
    ] {
        match map.get(field) {
            None => errors.push(format!("missing field: {field}")),
            Some(value) if !check(value) => {
                errors.push(format!("field {field} must be {kind}"));
            }
            Some(_) => {}
        }
    }
    let submit = map.get("submit").and_then(Value::as_bool).unwrap_or(false);
    if errors.is_empty() {
        json!({ "error": "", "submit": submit })
    } else {
        json!({ "error": errors.join("; "), "submit": false })
    }
}

// ============================================================
// U3 · pkg.sweng.test_unit（判定只取退出码与输出文本，零模型）
// ============================================================

/// U3：容器内跑测试命令并解析——`{pass, failures[, evidence]}`；pass 只取
/// 进程退出码，failures=输出末 10 非空行（截断）。命令源 = 入参指定或环境
/// 声明 test_command（`default_test_command`）；双缺 → fail-closed 观察。
///
/// 红队 W2-C10：`review` 给定时（图契约声明 tester consumes `review_report`，
/// sop_full reviewer→tester 边）评审结论对判定有因果——此前分发表完全不传
/// 入边工件：类型化数据边制造「评审已接入」的假象，approved=false 对订单判定
/// 零因果（F4D 实盘：reviewer approved=false 附两条问题、tester 退出码 0 →
/// pass=true 照常 qualified，多 agent 评审环结构上装饰性）。现语义：
/// - `approved=false` → 一票否决：容器退出码 0 亦翻假，failures 点名评审
///   问题（「review issue: …」前置，退出码失败行照录）；
/// - `approved=true` → 维持退出码判定，证据摘要留评审面（approved+issues）；
/// - 载荷缺 `approved` 布尔 = 类型化数据边失信，fail-closed 按否决处置并点名。
///
/// 图未声明 review 边（手写计划面）= 纯退出码判据，`review=None` 行为不变。
///
/// 红队 W2-C49：`evidence_dir` 给定时**无论成败**一律留档——`test.log`（完整
/// 观察文本，含 `__AF_RC` 标记与 `exit=` 尾行；保真上限=容器执行载体的 12k
/// 双端截断契约，截断与否如实记入摘要）+ `test-evidence.json`（命令/退出码/
/// 判定/体量口径）。旧行为 pass=true 时把观察整体丢弃、drive 相又不建节点
/// 目录——终态 pass 与外部评审结论相悖时订单内无任何日志可判。留档失败不翻
/// 判定，但在 envelope `evidence.error` 显形（不静默）。
pub fn test_unit(
    bridge: &dyn HostBridge,
    env: &TbEnv,
    default_test_command: &str,
    test_cmd: Option<&str>,
    review: Option<&Value>,
    evidence_dir: Option<&std::path::Path>,
) -> Value {
    let cmd = test_cmd
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_test_command.trim());
    if cmd.is_empty() {
        return json!({ "pass": false, "failures": ["no test command available"] });
    }
    let text = container_exec(bridge, env, cmd, 300);
    let (body, rc) = split_observation(&text);
    let mut pass = rc == Some(0);
    // pass → failures 收空（无失败可报）；不 pass → 输出末 10 非空行。
    let mut failures: Vec<String> = if pass {
        Vec::new()
    } else {
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    };
    if failures.len() > 10 {
        failures = failures.split_off(failures.len() - 10);
    }
    // 红队 W2-C10：评审门——approved=false（或契约失信）一票否决，问题点名
    // 前置（截断后的退出码失败行照录，不吞任一界面）。
    let review_summary = review.map(|r| {
        let approved = r.get("approved").and_then(Value::as_bool);
        let issues: Vec<String> = r
            .get("issues")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if approved == Some(true) {
            json!({ "approved": true, "issues": issues })
        } else {
            pass = false;
            let mut named = vec![match approved {
                Some(false) => {
                    "review 否决：评审员未放行（approved=false），测试退出码判定作废".to_string()
                }
                _ => "review 契约失信：缺 approved 布尔字段，fail-closed 否决".to_string(),
            }];
            named.extend(issues.iter().map(|i| format!("review issue: {i}")));
            let mut merged = named;
            merged.append(&mut failures);
            failures = merged;
            json!({ "approved": approved.unwrap_or(false), "issues": issues })
        }
    });
    let truncated = text.contains("...[output truncated:");
    let evidence = evidence_dir.map(|dir| -> Value {
        let summary = json!({
            "command": cmd,
            "exit_code": rc,
            "pass": pass,
            "observation_bytes": text.len(),
            "observation_truncated": truncated,
            "observation_cap_bytes": 12_000usize,
            "review": review_summary.clone(),
        });
        let write = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(dir)?;
            std::fs::write(dir.join("test.log"), &text)?;
            std::fs::write(
                dir.join("test-evidence.json"),
                serde_json::to_string_pretty(&summary).unwrap_or_default(),
            )?;
            Ok(())
        })();
        match write {
            Ok(()) => json!({
                "dir": dir.file_name().map(|n| n.to_string_lossy().into_owned()),
                "log": "test.log",
                "summary": "test-evidence.json",
                "exit_code": rc,
                "observation_truncated": truncated,
                "observation_bytes": text.len(),
            }),
            Err(err) => json!({ "error": format!("判据留档失败: {err}") }),
        }
    });
    let mut out = json!({ "pass": pass, "failures": failures });
    if let Some(ev) = evidence {
        out["evidence"] = ev;
    }
    out
}

// ============================================================
// U4 · pkg.sweagent.human_gate（v1 fail-closed 最小实现）
// ============================================================

/// 人工介入开关包配置（缺省 false；装载面——真工位接线推 v2）。
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HumanGateConfig {
    /// 是否请求人工介入（v1 读了也不打扰——fail-closed，见 [`human_gate`]）。
    #[serde(default)]
    pub enabled: bool,
}

/// U4：fail-closed 恒 `{ask:false}`（裁决17/设计开放问题 4 已批的 v1 口径：
/// 不请求人工，环走默认分支；`enabled=true` 无 af-station 真工位接线时同样
/// 走默认分支——不虚报 ask 能力）。判定不涉模型。
pub fn human_gate(_config: &HumanGateConfig, _observation: &Value) -> Value {
    json!({ "ask": false })
}
