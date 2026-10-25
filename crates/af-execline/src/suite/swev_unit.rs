//! SwevExecuteUnit——swe-v 单题真实执行单元（af-bench 吸收方案 §5-P4，
//! `run_swev` 单题执行体的结构重组落点，方案 §1.1"结构重组"行）。
//!
//! 分相执行（§2.4 / main.rs:625-914 结构）：
//! `setup（任务装载+容器）→ run（AgentRunner 缝）→ gate（记账非门控）
//! → verify（P3 判分链）→ 行落盘 → cleanup（RAII guard，Drop → docker rm -f）`。
//!
//! 语句级对照清单（方案 §1.3，评审按此逐段比对）：
//! - 任务装载 ← main.rs:640（load_swev_tasks）+ task_json 路径 ← main.rs:850；
//! - setup ← main.rs:652（swev::setup，P2 已挂 bridge）；
//! - setup_failed 行 ← main.rs:654-677（字段齐备含 null）；
//! - bash 工具护栏 ← main.rs:700-735（exec + 反考古注入 + 重复命令 ≥3 拦截）；
//! - AgentSpec 组装 ← main.rs:736-759；采样覆写 ← main.rs:633-636；
//! - attempt 目录/新建不覆写 ← main.rs:698, 760-766（D-20：分配在调用方/驱动层）；
//! - run（AgentRunner 缝）+ http_retries ← main.rs:776-800；
//! - run_failed 行 ← main.rs:818-845（记实际目录 + cleanup）；
//! - agent_wall_ms ← main.rs:688, 849（run 相时长，不含 setup/verify）；
//! - gate 计算 ← main.rs:801-813（no_patch/test_edit，只记账）；
//! - verify ← main.rs:852-853（P3 swev::verify，指纹门+三件套+judge 子进程）；
//! - 18 键 legacy 行 + v1 投影 ← main.rs:861-895（双轨同源 ExecutionFacts）。
//!
//! 语义等价、机制升级两点（方案明文）：cleanup 由显式调用改 RAII guard
//! （§2.4 防新增早退路径漏清容器）；订单四步生命周期（bench_order）不搬——
//! P5 由 driver 引擎 op 承接。清理必达 + setup/run 失败行语义保留 + gate
//! 不接引擎 close 门，三条 §2.4 约束全部落位。

use std::sync::{Arc, Mutex};
use std::time::Instant;

use af_harness::{AgentSpec, ContextTemplate, ToolDecl};
use af_workorder::WorkArtifact;
use serde_json::Value;

use crate::agent::{AgentLaunch, AgentRunFailure, AgentRunOutcome};
use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::hostbridge::HostBridge;
use crate::judge::JudgeLocator;
use crate::record::ExecutionFacts;
use crate::suite::swev::{self, ARCHAEOLOGY_PATTERNS};
use crate::suite::tb::{self, ContainerGuard, TbEnv};

/// swe-v 单元静态声明（方案 §2.2"换一档实验要改代码的东西进这里"，
/// 随 unit_version 版本化，对齐契约 §1.2 unit.json）。缺省值 = fix2 代际
/// af-bench 现值（§2.2 表列：采样覆写/两段提示词/Policy 250 步·$3/判分锚）。
#[derive(Debug, Clone)]
pub struct SwevUnitSpec {
    /// profile 名（→ EndpointSpec + key_cmd，P1 已迁 profile() 表）。
    pub profile: String,
    /// 采样覆写（main.rs:634-636）：温度（毫度）0。
    pub temperature_milli: u32,
    /// top_p（毫度）1.0 = 1000。
    pub top_p_milli: u32,
    /// max_tokens 32000（审计 D3：不吃协议默认）。
    pub max_tokens: u32,
    /// system 一行角色（main.rs:543；G4 §3.2-3 装载化：从 &'static str 改
    /// String 持有——包资产 `pkg.sweng.swev_system` 经 pkg::prompts 装载后
    /// 整体替换，fix2 缺省仍字节等于冻结常量）。
    pub system_prompt: String,
    /// instance 纪律模板（main.rs:548-566；装载化同上）。
    pub instance_protocol: String,
    /// Policy：1 turn = 1 模型调用，250 步（main.rs:748）。
    pub max_turns: u32,
    /// 完成信号词（main.rs:750）。
    pub completion_signal: &'static str,
    /// 连续无工具轮上限（main.rs:751）。
    pub tool_free_turn_limit: u32,
    /// 单题成本上限 $3（main.rs:787）。
    pub spend_cap_cents: i64,
    /// 工具单次开销（main.rs:788）。
    pub tool_spend_cents: i64,
    /// 结果代号（main.rs:871）。
    pub scaffold: &'static str,
    /// reasoning 档位显式声明（main.rs:872）。
    pub reasoning_tier: &'static str,
    /// 判分器定位与指纹锚（§3.2：装载期门 + verify 入口门双重防线）。
    pub judge: JudgeLocator,
    /// 单元版本（v1 行 unit_version；契约 §1.2 对齐）。
    pub unit_version: &'static str,
}

impl SwevUnitSpec {
    /// fix2 代际缺省（值 = af-bench main.rs 现值）。
    pub fn fix2(profile: impl Into<String>, judge: JudgeLocator) -> Self {
        Self {
            profile: profile.into(),
            temperature_milli: 0,
            top_p_milli: 1_000,
            max_tokens: 32_000,
            system_prompt: crate::suite::swev::SWEV_SYSTEM_PROMPT.into(),
            instance_protocol: crate::suite::swev::SWEV_INSTANCE_PROTOCOL.into(),
            max_turns: 250,
            completion_signal: "TASK_COMPLETE",
            tool_free_turn_limit: 3,
            spend_cap_cents: 300,
            tool_spend_cents: 1,
            scaffold: "v3",
            reasoning_tier: "off",
            judge,
            unit_version: "0.1.0",
        }
    }
}

/// swe-v 单题执行单元：持两个可注入缝构造（§2.3）——HostBridge（容器宿主）
/// 与 AgentRunner（模型执行）。Arc 形态见 agent.rs 文档（af-harness 'static
/// 约束）。
pub struct SwevExecuteUnit {
    spec: SwevUnitSpec,
    bridge: Arc<dyn HostBridge + Send + Sync>,
    runner: Arc<dyn crate::agent::AgentRunner>,
}

impl std::fmt::Debug for SwevExecuteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwevExecuteUnit")
            .field("unit_id", &self.unit_id())
            .field("profile", &self.spec.profile)
            .finish_non_exhaustive()
    }
}

impl SwevExecuteUnit {
    /// 装载：判分指纹门（§3.2-2 双重防线的装载期一道，与 verify 入口门互备）
    /// + profile 可解析——都 fail-loud，拒在起容器/调模型之前。
    pub fn new(
        spec: SwevUnitSpec,
        bridge: Arc<dyn HostBridge + Send + Sync>,
        runner: Arc<dyn crate::agent::AgentRunner>,
    ) -> Result<Self, ExeclineError> {
        spec.judge.check_fingerprint()?;
        crate::profile::profile(&spec.profile).map_err(|err| ExeclineError::UnitFailed {
            detail: format!("装载失败：{err}"),
        })?;
        Ok(Self {
            spec,
            bridge,
            runner,
        })
    }
}

impl ExecuteUnit for SwevExecuteUnit {
    fn unit_id(&self) -> &str {
        "swev-unit"
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        let profile = crate::profile::profile(&self.spec.profile).map_err(|err| {
            ExeclineError::UnitFailed {
                detail: format!("profile 解析失败: {err}"),
            }
        })?;
        // ---- setup 相（main.rs:640/652 + 850）----
        // 任务装载失败 = 供给面失败（题库根错/任务包缺），落 setup_failed 行后
        // Err（订单侧不停终态，EX3001 可重试——§2.4；af-bench 原为批级 ? 中止，
        // 单元化后按 F2 分类落行，登记 RESULT-P4.md §3）。
        let tasks = swev::load_swev_tasks(ctx.data_root, &[ctx.task_id.to_string()])
            .map_err(|err| self.setup_failed_row(ctx, &err))?;
        let Some(task) = tasks.into_iter().next() else {
            let err = format!("任务包空: {}", ctx.task_id);
            return Err(self.setup_failed_row(ctx, &err));
        };
        let task_json = ctx.data_root.join(format!("{}.json", ctx.task_id));
        let env = swev::setup(
            self.bridge.as_ref(),
            ctx.host_alias,
            &task,
            &self.spec.profile,
        )
        .map_err(|err| self.setup_failed_row(ctx, &err))?;
        let env = Arc::new(env);
        // cleanup 必达（§2.4）：RAII guard，任何早退路径 Drop → docker rm -f。
        let _guard = ContainerGuard {
            bridge: self.bridge.clone(),
            env: Some(env.clone()),
        };
        // ---- run 相（main.rs:698/760-798）----
        let attempt = attempt_from_dir(ctx.attempt_dir);
        let attempt_dir = ctx.attempt_dir;
        // D-20 项 1：只新建、不删除——旧目录是判分复核的证据，绝不覆写。
        std::fs::create_dir_all(attempt_dir).map_err(|err| ExeclineError::UnitFailed {
            detail: format!("建 run 目录失败: {err}"),
        })?;
        let run_started = Instant::now();
        let bash = self.bash_tool(env.clone());
        let launch = AgentLaunch {
            attempt_dir,
            run_id: format!("{}-a{}", task.instance_id, attempt),
            user_message: format!("{}\n\n{}", task.prompt(), self.spec.instance_protocol),
            spec: self.agent_spec(&profile.endpoint, env.workdir())?,
            bash,
            key_cmd: profile.key_cmd,
            spend_cap_cents: self.spec.spend_cap_cents,
            tool_spend_cents: self.spec.tool_spend_cents,
            budget_ledger_cents: self.spec.spend_cap_cents,
        };
        let mut http_retries: u64 = 0;
        let report = match self.runner.run(launch) {
            Ok(AgentRunOutcome {
                report,
                http_retries: retries,
            }) => {
                http_retries += retries;
                report
            }
            Err(AgentRunFailure {
                message,
                http_retries: retries,
            }) => {
                http_retries += retries;
                // run_failed 行（main.rs:818-845）：记实际目录 + 累计重试 + 失败
                // 文案；行落盘后 Err（订单侧不停终态）。
                let row = crate::record::swev_run_failed_row(
                    &task.instance_id,
                    &self.spec.profile,
                    ctx.trial,
                    attempt_dir,
                    http_retries,
                    &message,
                );
                return Err(crate::record::fail_with_legacy_row(
                    ctx,
                    &row,
                    &format!("run 失败: {message}"),
                ));
            }
        };
        // gate 相（main.rs:801-813）：完成度记账非门控——no_patch/test_edit 落行，
        // 不入重试、不阻断判分。
        let diff = swev::capture_diff(self.bridge.as_ref(), ctx.host_alias, &env, attempt_dir)
            .unwrap_or_default();
        let gate = gate_of(&diff);
        // agent_wall_ms（main.rs:688/849）：run 相时长（key 现取 + 模型 + gate 取证），
        // 不含 setup/verify。
        let agent_wall_ms = run_started.elapsed().as_millis() as u64;
        // ---- verify 相（main.rs:852-853：P3 判分链原样）----
        let (resolved, note, eval_sha256) = swev::verify(
            self.bridge.as_ref(),
            &self.spec.judge,
            ctx.host_alias,
            &env,
            &task,
            attempt_dir,
            &task_json,
        );
        // ---- 行落盘相（main.rs:861-895：双轨同源）----
        let facts = ExecutionFacts {
            suite: "swe-v",
            task_id: task.instance_id.clone(),
            profile: self.spec.profile.clone(),
            trial: ctx.trial,
            reward: resolved,
            status: crate::suite::status_label(&report.status),
            scaffold: self.spec.scaffold,
            reasoning_tier: self.spec.reasoning_tier,
            run_dir: Some(attempt_dir.display().to_string()),
            eval_sha256,
            gate: gate.into(),
            attempts: 1,
            http_retries,
            turns: report.turns,
            model_calls: report.model_calls,
            tokens: report.tokens,
            agent_wall_ms,
            verifier_note: note,
        };
        let legacy_row = crate::record::swev_legacy_row(&facts);
        let v1_row = crate::record::swev_v1_row(
            &facts,
            ctx.order_id,
            ctx.line_id,
            ctx.host_alias,
            self.unit_id(),
            self.spec.unit_version,
        );
        crate::record::append_legacy_row(ctx, &legacy_row)?;
        self.append_v1_row(ctx, &v1_row)?;
        // 判分三件套实际路径（§2.2 evidence：只记在盘文件——早期失败臂缺件不虚列）。
        let evidence = [
            task_json,
            attempt_dir.join("eval.log"),
            attempt_dir.join("agent.diff"),
        ]
        .into_iter()
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
        Ok(ExecOutput {
            // resolved=true → 合格终态工件（引擎 close 门材料；gate 不接 close 门，
            // §2.4——no_patch/test_edit 行照落，工件资格只看判分）。
            artifacts: vec![WorkArtifact {
                artifact_id: format!("swev-{}", task.instance_id),
                is_final: true,
                qualified: resolved == Some(true),
            }],
            outputs: evidence.clone(),
            note: facts.verifier_note.chars().take(300).collect(),
            verdict: resolved,
            record: Some(v1_row),
            evidence,
        })
    }
}

impl SwevExecuteUnit {
    /// setup_failed 行落盘 + 转 EX3001（main.rs:654-677：字段齐备含 null；
    /// §2.4 订单侧不停终态——可重试）。失败路径落盘共用件 =
    /// [`crate::record::fail_with_legacy_row`]（P5 重构抽取，tb 单元同款）。
    fn setup_failed_row(&self, ctx: &ExecContext<'_>, err: &str) -> ExeclineError {
        let row =
            crate::record::swev_setup_failed_row(ctx.task_id, &self.spec.profile, ctx.trial, err);
        crate::record::fail_with_legacy_row(ctx, &row, &format!("setup 失败: {err}"))
    }

    /// v1 行落盘（{{output_dir}}/records.jsonl 流；方案 §1.4；swe-v 独有——
    /// TB 无 v1 投影）。
    fn append_v1_row(&self, ctx: &ExecContext<'_>, row: &Value) -> Result<(), ExeclineError> {
        ctx.record_sink
            .append_v1(&row.to_string())
            .map_err(|err| ExeclineError::UnitFailed {
                detail: format!("写 records 失败: {err}"),
            })
    }

    /// AgentSpec 组装（main.rs:736-759 + 采样覆写 main.rs:633-636）。
    fn agent_spec(
        &self,
        endpoint: &af_harness::EndpointSpec,
        workdir: &str,
    ) -> Result<AgentSpec, ExeclineError> {
        // fix2 采样参数对齐（mini-swe-agent 榜单口径，只作用于 swe-v 线）：
        // temperature 0.0 / top_p 1.0 显式写入请求体；max_tokens 显式 32000
        // （不吃协议默认）。值出自 spec（换档位=换 spec）。
        let mut endpoint = endpoint.clone();
        endpoint.temperature_milli = Some(self.spec.temperature_milli);
        endpoint.top_p_milli = Some(self.spec.top_p_milli);
        endpoint.max_tokens = Some(self.spec.max_tokens);
        let tool_decl =
            serde_json::from_value::<ToolDecl>(tb::bash_tool_decl(workdir)).map_err(|err| {
                ExeclineError::UnitFailed {
                    detail: format!("bash 工具声明: {err}"),
                }
            })?;
        Ok(AgentSpec {
            endpoint,
            context_template: ContextTemplate {
                system: self.spec.system_prompt.clone(),
                slots: Vec::new(),
            },
            toolset: vec![tool_decl],
            // 1 turn = 1 模型调用（与 mini step 同义）；组装共用件 =
            // [`crate::agent::llm_cents_policy`]（P5 重构抽取，tb 单元同款）。
            policy: crate::agent::llm_cents_policy(
                self.spec.max_turns,
                self.spec.completion_signal,
                self.spec.tool_free_turn_limit,
            ),
        })
    }

    /// bash 工具闭包（main.rs:700-735）：容器执行（经 bridge）+ 反考古护栏 +
    /// 重复命令拦截。闭包整体 'static + Send + Sync（af-harness 工具函数体约束，
    /// run.rs:53-55）——环境句柄与桥以 Arc 拥有，最近命令窗口以 Mutex 持有。
    fn bash_tool(&self, env: Arc<TbEnv>) -> Arc<dyn Fn(&str) -> String + Send + Sync> {
        let bridge = self.bridge.clone();
        let recent = Arc::new(Mutex::new(Vec::<String>::new()));
        Arc::new(move |command: &str| {
            let out = env.exec(bridge.as_ref(), command);
            let mut hist = recent.lock().unwrap_or_else(|e| e.into_inner());
            bash_guard_wrap(command, out, &mut hist)
        })
    }
}

/// bash 工具护栏包装（af-bench main.rs:710-734 原样语义）：
/// 反考古命中即在输出前注入护栏（51/145 run 单例烧 10-20 轮且从不收敛）；
/// 重复命令计入最近 6 条窗口，第 3 次起拦截教学。
pub(crate) fn bash_guard_wrap(command: &str, out: String, recent: &mut Vec<String>) -> String {
    let mut out = out;
    if ARCHAEOLOGY_PATTERNS.iter().any(|p| command.contains(p)) {
        out = format!(
            "[harness] Searching for an upstream fix (git archaeology / full-disk search) is unavailable in this environment — the upstream fix is not in this repository, this line of work never pays off. Implement the fix yourself with the code in /testbed.\n{out}"
        );
    }
    let repeats = recent.iter().filter(|c| c.as_str() == command).count();
    recent.push(command.to_string());
    if recent.len() > 6 {
        recent.remove(0);
    }
    if repeats >= 2 {
        return format!(
            "[harness] You have issued this exact command {} times. Repeating it will not make progress — try a different approach, or finish with your best current fix.\n{}",
            repeats + 1, out
        );
    }
    out
}

/// gate 计算（af-bench main.rs:806-813 原样）：空补丁 no_patch / 碰测试文件
/// test_edit / 其余 ok。完成度记账非门控（审计 D5 关闭 gate 重试，§2.4）。
fn gate_of(diff: &str) -> &'static str {
    if diff.trim().is_empty() {
        "no_patch"
    } else if !swev::diff_touches_tests(diff).is_empty() {
        "test_edit"
    } else {
        "ok"
    }
}

/// 从 run 目录名解析 attempt 序号（swev-<task>-<profile>-a<N>，next_attempt_index
/// 的命名约定）：目录由调用方/驱动层分配（§2.2），run_id 的 `-a<N>` 后缀取自
/// 目录名（main.rs:778）。非约定命名 → 1（fix2 口径 attempt 恒 1，行内
/// attempts 字段本就不随目录序号变；登记 RESULT-P4.md §3）。
fn attempt_from_dir(attempt_dir: &std::path::Path) -> u32 {
    attempt_dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit_once("-a"))
        .and_then(|(_, num)| num.parse().ok())
        .unwrap_or(1)
}

// 容器清理 RAII guard：P5 起定居 `crate::suite::tb::ContainerGuard`
// （swev/tb 两单元共用），本模块按上文体注引用。

#[cfg(test)]
mod tests {
    use super::*;

    /// 护栏纯函数：普通命令透传；反考古注入前缀；第 3 次同命令拦截；
    /// 窗口滑出（>6 条丢最老）后同名命令重新计数。
    #[test]
    fn bash_guard_wrap_semantics() {
        let mut recent = Vec::new();
        let out = bash_guard_wrap("echo hi", "out\nexit=0".into(), &mut recent);
        assert_eq!(out, "out\nexit=0");
        assert!(bash_guard_wrap("git fsck", "o".into(), &mut recent)
            .starts_with("[harness] Searching for an upstream fix"));
        // 第 3 次（repeats=2 前置命中）→ 教学包装，次数计数 = 3。
        bash_guard_wrap("dup", String::new(), &mut recent);
        let _ = bash_guard_wrap("dup", String::new(), &mut recent);
        let third = bash_guard_wrap("dup", "body".into(), &mut recent);
        assert!(third.starts_with("[harness] You have issued this exact command 3 times."));
        assert!(third.ends_with("body"), "原输出保留: {third}");
    }

    /// 窗口滑出：6 条窗口丢最老，同名命令滑出后重新计数（main.rs:725-727）。
    #[test]
    fn bash_guard_window_slides_after_six() {
        let mut recent = Vec::new();
        bash_guard_wrap("dup", String::new(), &mut recent);
        bash_guard_wrap("dup", String::new(), &mut recent);
        // 6 条异名命令把两条 dup 挤出窗口（push 后 >6 即丢最老）。
        for i in 0..6 {
            bash_guard_wrap(&format!("pad{i}"), String::new(), &mut recent);
        }
        assert_eq!(recent.len(), 6, "窗口恒 6: {recent:?}");
        assert!(
            !recent.contains(&"dup".to_string()),
            "dup 已滑出: {recent:?}"
        );
        let out = bash_guard_wrap("dup", "plain".into(), &mut recent);
        assert_eq!(out, "plain", "窗口滑出后同命令重新计数，不误拦");
    }

    /// attempt 目录名解析：约定命名取后缀序号；非约定命名回退 1。
    #[test]
    fn attempt_from_dir_follows_next_attempt_naming() {
        let dir = std::path::Path::new("/tmp/runs/swev-x-glm-cp-a3");
        assert_eq!(attempt_from_dir(dir), 3);
        assert_eq!(
            attempt_from_dir(std::path::Path::new("/tmp/runs/free-name")),
            1
        );
    }

    /// gate 三分支（main.rs:806-813）：空/纯空白 → no_patch；碰测试文件 →
    /// test_edit；纯源码补丁 → ok。
    #[test]
    fn gate_of_three_branches() {
        assert_eq!(gate_of(""), "no_patch");
        assert_eq!(gate_of("  \n\t "), "no_patch");
        assert_eq!(gate_of("diff --git a/x.py b/x.py\n"), "ok");
        assert_eq!(
            gate_of("diff --git a/x.py b/x.py\ndiff --git a/tox.ini b/tox.ini\n"),
            "test_edit"
        );
    }
}
