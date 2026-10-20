//! Run 循环。
//!
//! 每轮：渲染上下文 → 调模型 → 解析工具调用 → 副作用走 effect gateway，纯函数本地执行
//! → 写 trace → 止损 → 按轮扣预算。预算不足、止损、完成信号、无工具轮上限、轮次上限都停。
//! 每轮开始前如果余额已经不够，模型不会被调用。
//!
//! 断点 [`Breakpoint::BeforeTurnCommit`] 在副作用已经落进网关、运行行还没提交时返回。
//! 续跑重放同一幂等键，不把外部操作再做一次。

use crate::authz::{issue_run_credential, RunCredential};
use crate::budget::{BudgetBinding, BudgetRecord, BudgetState};
use crate::clock::{Clock, SharedClock};
use crate::endpoint::{complete_once, effect_label, EndpointKey, ModelMessage, TranscriptItem};
use crate::error::{Error, Result};
use crate::render::render;
use crate::spec::{AgentSpec, EffectClass, ToolDecl};
use crate::store::{OpenStatus, RunRecord, RunStore};
use af_effect_gateway::{Gateway, GatewayError, IntentKey, Session};
use af_stop::{Decision, Event, StopEngine, StopPolicy};
use af_trace::{OpenOutcome, Trace, TraceConfig};
use af_workorder::WorkOrder;
use biscuit_auth::{Biscuit, PublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// 测试注入的提交前断点。生产路径用 [`Breakpoint::None`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Breakpoint {
    #[default]
    None,
    /// 本轮副作用已经执行，sqlite 运行行还没更新。
    BeforeTurnCommit,
}

/// 调用方注册的工具实现。规格决定路由，这里只提供函数体。
pub struct ToolRegistry {
    handlers: HashMap<String, ToolHandler>,
}

type ToolHandler = Arc<dyn Fn(&Value) -> std::result::Result<Value, ToolFailure> + Send + Sync>;

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: impl Into<String>, handler: F)
    where
        F: Fn(&Value) -> std::result::Result<Value, ToolFailure> + Send + Sync + 'static,
    {
        self.handlers.insert(name.into(), Arc::new(handler));
    }

    fn invoke(&self, name: &str, args: &Value) -> std::result::Result<Value, ToolFailure> {
        match self.handlers.get(name) {
            Some(handler) => handler(args),
            None => Err(ToolFailure {
                error_code: "E_NO_HANDLER".into(),
                message: format!("没有注册工具 {name}"),
            }),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 工具函数体的失败。这是给止损引擎的事件，不是执行器故障。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFailure {
    pub error_code: String,
    pub message: String,
}

/// 401 时的密钥重取器：重跑一次密钥解析命令拿新 key。明文只经内存进请求头，
/// 不进错误文本、不进 trace。返回 Err 表示本次重取失败（照常失败，不无限重试）。
pub type KeyRefresher = Arc<dyn Fn() -> std::result::Result<String, String> + Send + Sync>;

/// 一次 `run` 的输入。父凭证和工单由调用方持有，执行器不把它们落库。
pub struct Launch<'a> {
    pub spec: AgentSpec,
    pub run_id: String,
    pub vars: BTreeMap<String, String>,
    pub user_message: String,
    pub key: EndpointKey,
    /// Some 时端点 401（invalid token，如 mcode 令牌 1h 寿命撞长题）重取密钥重试
    /// 该请求一次。可靠性修复，不改测量语义：不重置对话/预算/轮次。
    pub key_refresh: Option<KeyRefresher>,
    pub parent: &'a Biscuit,
    pub public_key: &'a PublicKey,
    pub namespace: String,
    pub spend_cap_cents: i64,
    pub tool_spend_cents: i64,
    pub budget: BudgetBinding,
    pub order: &'a WorkOrder,
    pub tools: ToolRegistry,
    pub stop_policy: StopPolicy,
    pub breakpoint: Breakpoint,
}

/// 循环的正常停止。传输失败、声明不符、提交前中断不在这里。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunStatus {
    Completed {
        signal: String,
    },
    MaxTurns,
    ToolFreeLimit,
    BudgetExhausted {
        fault: crate::budget::BudgetFault,
    },
    Stopped {
        decision: Decision,
    },
    Unauthorized {
        tool: String,
        fact_present: bool,
        code: crate::authz::AuthzCode,
        detail: String,
    },
}

/// 一次 `run` 的结果。已经终态的 Run 再次调用时原样返回，不再调模型。
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub run_id: String,
    pub status: RunStatus,
    pub turns: u32,
    pub model_calls: u64,
    pub effect_replays: u64,
    pub effect_fresh: u64,
    pub transcript: Vec<TranscriptItem>,
    pub whitelist: Vec<String>,
    /// 供给侧 token 三本账（输入/输出/缓存读），跨轮累计。
    pub tokens: TokenTotals,
}

/// 一次 Run 的 token 总量。网关侧独立计量，误差与供应商账单对账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenTotals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
}

/// 打开后的执行器。一个目录一把库、一个网关、一份 trace。
pub struct Opened {
    status: OpenStatus,
    store: RunStore,
    gateway: Gateway,
    session: Session,
    trace: Arc<Trace>,
    clock: Arc<dyn Clock>,
    http: crate::http::UreqClient,
}

impl Opened {
    /// 用系统钟打开 `dir`。目录不存在会创建。
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_clock(dir, crate::clock::SystemClock::new())
    }

    /// 同 [`Opened::open`]，时刻走调用方注入的钟。
    pub fn open_with_clock(dir: impl AsRef<Path>, clock: impl Clock + 'static) -> Result<Self> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|err| Error::Recipe(format!("创建目录: {err}")))?;
        let clock: Arc<dyn Clock> = Arc::new(clock);
        let (store, status) = RunStore::open(&dir.join("harness.db"))?;
        let shared = SharedClock(Arc::clone(&clock));
        let gateway =
            Gateway::open_with_clock(dir.join("effects.db"), Duration::ZERO, shared.clone())
                .map_err(|err| Error::Gateway(Box::new(err)))?;
        let (trace, trace_status) = Trace::open_with_clock(trace_config(dir), shared)
            .map_err(|err| Error::Trace(err.to_string()))?;
        if !status_matches(status, trace_status) {
            return Err(Error::DeclarationMismatch(format!(
                "harness.db 的打开结果是 {status:?}，trace 是 {trace_status:?}"
            )));
        }
        let session = match status {
            OpenStatus::Created => gateway.attach(),
            OpenStatus::AlreadyDeclared => gateway.resume(),
        }
        .map_err(|err| Error::Gateway(Box::new(err)))?;
        Ok(Self {
            status,
            store,
            gateway,
            session,
            trace,
            clock,
            http: crate::http::UreqClient::new(),
        })
    }

    pub fn status(&self) -> OpenStatus {
        self.status
    }

    /// 本 Run 到目前为止 HTTP 层累计的退避重试次数（429/5xx 供给面瞬断观测口径）。
    pub fn http_retries(&self) -> u64 {
        self.http.retries_done()
    }

    /// 启动或续跑。终态 Run 直接返回已落库的报告。
    pub fn run(&self, launch: Launch<'_>) -> Result<RunReport> {
        launch.spec.validate()?;
        if launch.run_id.is_empty() || launch.namespace.is_empty() {
            return Err(Error::InvalidSpec("run_id 与 namespace 不能为空".into()));
        }
        if launch.order.id().is_empty() {
            return Err(Error::Order("工单 id 为空".into()));
        }
        if launch.budget.dimension() != launch.spec.policy.budget_ref.dimension
            || launch.budget.unit() != launch.spec.policy.budget_ref.unit
        {
            return Err(Error::InvalidSpec(
                "预算绑定的维度或单位与规格 budget_ref 不一致".into(),
            ));
        }
        let _ = render(&launch.spec.context_template, &launch.vars)?;
        match self.bind(&launch)? {
            Bound::Done(report) => Ok(report),
            Bound::Live(mut live) => self.drive(&launch, &mut live),
        }
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        let _ = self.trace.shutdown();
    }
}

fn trace_config(dir: &Path) -> TraceConfig {
    TraceConfig {
        critical_capacity: 4096,
        normal_capacity: 4096,
        critical_consume_delay_us: 0,
        normal_consume_delay_us: 0,
        dir: dir.to_path_buf(),
        otel_endpoint: None,
        otel_batch_size: 512,
        anchor: af_trace::AnchorSettings::default(),
    }
}

fn status_matches(store: OpenStatus, trace: OpenOutcome) -> bool {
    matches!(
        (store, trace),
        (OpenStatus::Created, OpenOutcome::Created)
            | (OpenStatus::AlreadyDeclared, OpenOutcome::AlreadyDeclared)
    )
}

enum Bound {
    Done(RunReport),
    Live(Box<Live>),
}

struct Live {
    cred: RunCredential,
    row: RunRecord,
    engine: StopEngine,
    budget: BudgetState,
    transcript: Vec<TranscriptItem>,
    /// 当前生效密钥。401 重取成功后在此换新，后续轮次直接用（旧 key 已死不再碰）。
    key: EndpointKey,
}

enum TurnOutcome {
    Progress,
    Terminal(RunStatus),
    Interrupted,
}

struct ToolPass {
    denied: Option<RunStatus>,
    latched: Option<Decision>,
    /// 本轮每个已执行工具调用的输出文本（按调用顺序，含失败调用的错误文本）。
    /// fix2 完成判定语义：完成信号只认这里的首行，不再认助手叙述文本。
    outputs: Vec<String>,
}

/// 工具输出统一取文本：字符串原样，其余 JSON 序列化。
fn output_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// fix2 无工具轮教学反馈：第一轮就纠偏，不再静默数满 tool_free 倒计时
/// （mini-swe-agent FormatError 教学反馈的同源思路，消息级续跑而非整轮重跑）。
const TOOL_FREE_FEEDBACK: &str = "[Harness feedback] Your previous message contained no tool call. Progress requires acting through tool calls — issue your next tool call now. Repeated replies without tool calls will terminate this run.";

/// fix2 finish_reason=length 专项反馈：被截断且无工具调用是死态，转成可恢复态。
const LENGTH_FEEDBACK: &str = "[Harness feedback] Your previous response was truncated before it produced a tool call (finish reason: length). Be more concise and issue a tool call in this turn.";

enum Dispatch {
    Done(Done),
    Denied(RunStatus),
}

struct Done {
    call_id: String,
    name: String,
    output: Value,
    is_error: bool,
    error_code: String,
    effect: EffectClass,
}

#[derive(Debug, Serialize, Deserialize)]
struct EffectReceipt {
    output: Value,
}

impl Opened {
    fn bind(&self, launch: &Launch<'_>) -> Result<Bound> {
        let names: Vec<String> = launch
            .spec
            .toolset
            .iter()
            .map(|tool| tool.name.clone())
            .collect();
        let cred = issue_run_credential(
            launch.parent,
            launch.public_key,
            &names,
            &launch.namespace,
            launch.spend_cap_cents,
            launch.tool_spend_cents,
        )?;
        if let Some(existing) = self.store.load(&launch.run_id)? {
            self.check_decl(&existing, launch, &cred)?;
            if let Some(status) = terminal_status(&existing)? {
                return Ok(Bound::Done(report_of(&existing, status)?));
            }
            return Ok(Bound::Live(Box::new(
                self.activate(existing, launch, cred)?,
            )));
        }
        let row = fresh_row(launch, &cred, self.clock.wall_ms())?;
        self.store.insert(&row)?;
        Ok(Bound::Live(Box::new(self.activate(row, launch, cred)?)))
    }

    fn activate(&self, row: RunRecord, launch: &Launch<'_>, cred: RunCredential) -> Result<Live> {
        let transcript = parse_stored(&row.transcript_json, "对话快照")?;
        let snap = parse_stored(&row.stop_state_json, "止损快照")?;
        let engine = StopEngine::from_snapshot(launch.stop_policy.clone(), snap)
            .map_err(|err| Error::DeclarationMismatch(format!("止损快照无法恢复: {err}")))?;
        let record: BudgetRecord = parse_stored(&row.budget_json, "预算")?;
        let budget = record.state()?;
        Ok(Live {
            cred,
            row,
            engine,
            budget,
            transcript,
            key: launch.key.clone(),
        })
    }

    fn check_decl(&self, row: &RunRecord, launch: &Launch<'_>, cred: &RunCredential) -> Result<()> {
        let stored_spec: AgentSpec = parse_stored(&row.spec_json, "规格")?;
        if stored_spec != launch.spec {
            return Err(mismatch("规格与已声明的 Run 不一致"));
        }
        let stored_vars: BTreeMap<String, String> = parse_stored(&row.vars_json, "变量")?;
        if stored_vars != launch.vars {
            return Err(mismatch("变量与已声明的 Run 不一致"));
        }
        if row.user_message != launch.user_message {
            return Err(mismatch("user_message 与已声明的 Run 不一致"));
        }
        if row.namespace != launch.namespace {
            return Err(mismatch("namespace 与已声明的 Run 不一致"));
        }
        if row.spend_cap_cents != launch.spend_cap_cents
            || row.tool_spend_cents != launch.tool_spend_cents
        {
            return Err(mismatch("花费上限与已声明的 Run 不一致"));
        }
        if row.order_id != launch.order.id() {
            return Err(mismatch("工单 id 与已声明的 Run 不一致"));
        }
        let stored_policy: Value = parse_stored(&row.stop_policy_json, "止损策略")?;
        if stored_policy != serde_json::to_value(&launch.stop_policy)? {
            return Err(mismatch("止损策略与已声明的 Run 不一致"));
        }
        let stored_whitelist: Vec<String> = parse_stored(&row.whitelist_json, "白名单")?;
        if stored_whitelist != cred.tools {
            return Err(mismatch("短期凭证白名单与已声明的 Run 不一致"));
        }
        let budget: BudgetRecord = parse_stored(&row.budget_json, "预算")?;
        budget.matches_launch(&launch.budget)?;
        Ok(())
    }

    fn drive(&self, launch: &Launch<'_>, live: &mut Live) -> Result<RunReport> {
        let max_turns = launch.spec.policy.max_turns;
        let per_turn = launch.spec.policy.budget_ref.per_turn;
        loop {
            if let Some(status) = terminal_status(&live.row)? {
                return report_of(&live.row, status);
            }
            if live.row.turn_index >= max_turns {
                return self.finish(live, RunStatus::MaxTurns);
            }
            if let Err(fault) = live.budget.probe(per_turn, self.clock.wall_ms()) {
                return self.finish(live, RunStatus::BudgetExhausted { fault });
            }
            match self.one_turn(launch, live)? {
                TurnOutcome::Terminal(status) => return self.finish(live, status),
                TurnOutcome::Interrupted => {
                    return Err(Error::Interrupted {
                        run_id: live.row.run_id.clone(),
                        turn: live.row.turn_index,
                    });
                }
                TurnOutcome::Progress => self.persist(live)?,
            }
        }
    }

    fn one_turn(&self, launch: &Launch<'_>, live: &mut Live) -> Result<TurnOutcome> {
        let per_turn = launch.spec.policy.budget_ref.per_turn;
        // fix2 收尾机制（失败轨迹分析 建议1）：轮次预算可见性。S1 实证 60 例 max_turns
        // 中 19 例补丁已过判分仍在"再验一组"——模型按无限预算规划验证。
        // 剩 ~10% 轮注入 gentle 提醒、剩 ~4% 轮注入 strong 警告，各恰好一次
        // （以 remaining 精确等于阈值触发，turn_index 单调故不会重发）。
        let max_turns = launch.spec.policy.max_turns;
        let gentle_left = turn_reminder_threshold(max_turns, 10, 5);
        let strong_left = turn_reminder_threshold(max_turns, 25, 2);
        let remaining = max_turns.saturating_sub(live.row.turn_index);
        if remaining == gentle_left {
            live.transcript.push(TranscriptItem::User {
                text: format!(
                    "[Harness feedback] Turn budget: {gentle_left} turns remain before this run is force-stopped. If your fix is already verified, submit now (print the completion token via a command). Do not start documentation, changelogs or broad re-verification."
                ),
            });
        }
        if remaining == strong_left {
            live.transcript.push(TranscriptItem::User {
                text: format!(
                    "[Harness feedback] FINAL warning: only {strong_left} turns remain. Submit your best current fix NOW."
                ),
            });
        }
        let system = render(&launch.spec.context_template, &launch.vars)?;
        let mut request = Vec::with_capacity(live.transcript.len() + 1);
        request.push(TranscriptItem::System { text: system });
        request.extend(live.transcript.iter().cloned());
        let tools = visible_tools(&launch.spec, &live.cred.tools);
        let message = match complete_once(
            &self.http,
            &launch.spec.endpoint,
            &live.key,
            &request,
            &tools,
        ) {
            Err(Error::Endpoint {
                status: 401,
                message,
            }) => self.retry_with_fresh_key(launch, live, &request, &tools, 401, message)?,
            other => other?,
        };
        live.row.model_calls = live.row.model_calls.saturating_add(1);
        self.trace.write_normal(
            "harness.model",
            json!({
                "run_id": live.row.run_id,
                "turn": live.row.turn_index,
                "model": launch.spec.endpoint.model,
                "finish": message.finish,
                "usage": message.usage.map(|u| json!({
                    "input_tokens": u.input_tokens,
                    "output_tokens": u.output_tokens,
                    "cache_read_tokens": u.cache_read_tokens,
                })),
            }),
        );
        let finish = message.finish.clone();
        live.transcript.push(TranscriptItem::Assistant {
            text: message.text,
            tool_calls: message.tool_calls.clone(),
            usage: message.usage,
        });
        let any_tool = !message.tool_calls.is_empty();
        let truncated = finish == "length" || finish == "max_tokens";
        let pass = self.run_tools(launch, live, &message.tool_calls)?;
        if any_tool {
            live.row.tool_free_streak = 0;
        } else if truncated {
            // fix2（失败轨迹分析 建议2）：截断产生的空轮**不计** tool_free 计数——
            // max_tokens 截断是供给面 bug，不得假冒"模型自愿停"的停止信号
            // （S1 实证 m31f-14096 被空轮触发的 tool_free_limit 误杀）。注入续写反馈。
            live.transcript.push(TranscriptItem::User {
                text: LENGTH_FEEDBACK.to_string(),
            });
        } else {
            live.row.tool_free_streak = live.row.tool_free_streak.saturating_add(1);
            // fix2：无工具轮不再静默倒计时——还不会因此停轮就立刻注入教学反馈。
            // 若本轮已到 tool_free 上限，下一轮判定直接停，不再注入。
            let limit = launch.spec.policy.stop.tool_free_turn_limit;
            if limit > 0 && live.row.tool_free_streak < limit {
                live.transcript.push(TranscriptItem::User {
                    text: TOOL_FREE_FEEDBACK.to_string(),
                });
            }
        }
        live.row.turn_index = live.row.turn_index.saturating_add(1);
        self.settle_turn(launch, live, per_turn, any_tool, pass)
    }

    /// 401（invalid token：mcode 令牌 1h 寿命撞 >1h 长题、OAuth 对被并发轮换）时的
    /// 单次重取重试。语义（可靠性修复，不改测量口径）：
    ///   · 只重跑一次密钥解析命令，只重试当前这一个请求——不重置对话/预算/轮次；
    ///   · 重取结果与旧 key 相同（或重取失败、无重取器）→ 照常失败，错误里注明；
    ///   · 重取到新 key → 换用新 key 重试一次，无论成败后续轮次都用新 key
    ///     （旧 key 已死，不再每轮撞 401）。新 key 明文只进请求头，不进错误文本。
    fn retry_with_fresh_key(
        &self,
        launch: &Launch<'_>,
        live: &mut Live,
        request: &[TranscriptItem],
        tools: &[ToolDecl],
        status: u16,
        message: String,
    ) -> Result<ModelMessage> {
        let fail = |message: String| Error::Endpoint { status, message };
        let Some(refresher) = &launch.key_refresh else {
            return Err(fail(message));
        };
        let fresh = match refresher() {
            Ok(key) => key,
            Err(err) => {
                return Err(fail(format!(
                    "{message}；[401 后重取密钥失败: {}]",
                    err.chars().take(160).collect::<String>()
                )));
            }
        };
        if fresh == live.key.expose() {
            return Err(fail(format!(
                "{message}；[401 后重取密钥与旧密钥相同，判定密钥面无变化，不再重试]"
            )));
        }
        let key = EndpointKey::new(fresh);
        self.trace.write_normal(
            "harness.key_refresh",
            json!({
                "run_id": live.row.run_id,
                "turn": live.row.turn_index,
                "reason": "endpoint_401",
                "retried": true,
            }),
        );
        let outcome = complete_once(&self.http, &launch.spec.endpoint, &key, request, tools);
        live.key = key;
        outcome
    }

    fn settle_turn(
        &self,
        launch: &Launch<'_>,
        live: &mut Live,
        per_turn: u64,
        any_tool: bool,
        pass: ToolPass,
    ) -> Result<TurnOutcome> {
        if let Err(fault) = live.budget.charge(per_turn, self.clock.wall_ms()) {
            if let Some(decision) = &pass.latched {
                self.trace_stop(&live.row, decision)?;
            }
            return Ok(TurnOutcome::Terminal(RunStatus::BudgetExhausted { fault }));
        }
        if let Some(status) = pass.denied {
            return Ok(TurnOutcome::Terminal(status));
        }
        if let Some(decision) = pass.latched {
            self.trace_stop(&live.row, &decision)?;
            return Ok(TurnOutcome::Terminal(RunStatus::Stopped { decision }));
        }
        // fix2 完成判定（mini-swe-agent has_finished 同源）：完成信号必须由一个
        // 真实执行过的工具调用产出——其输出文本的首行（trim 后）等于信号词。
        // 助手叙述文本里出现信号词（哪怕是"先别发 TASK_COMPLETE"）不再触发完成。
        if let Some(signal) =
            matched_signal(&pass.outputs, &launch.spec.policy.stop.completion_signals)
        {
            return Ok(TurnOutcome::Terminal(RunStatus::Completed { signal }));
        }
        let limit = launch.spec.policy.stop.tool_free_turn_limit;
        if !any_tool && (limit == 0 || live.row.tool_free_streak >= limit) {
            return Ok(TurnOutcome::Terminal(RunStatus::ToolFreeLimit));
        }
        if launch.breakpoint == Breakpoint::BeforeTurnCommit {
            return Ok(TurnOutcome::Interrupted);
        }
        Ok(TurnOutcome::Progress)
    }

    fn run_tools(
        &self,
        launch: &Launch<'_>,
        live: &mut Live,
        calls: &[crate::endpoint::ParsedToolCall],
    ) -> Result<ToolPass> {
        let mut latched = None;
        let mut denied = None;
        let mut outputs = Vec::new();
        for (index, call) in calls.iter().enumerate() {
            if denied.is_some() {
                break;
            }
            match self.dispatch(launch, live, index, call)? {
                Dispatch::Denied(status) => denied = Some(status),
                Dispatch::Done(done) => {
                    outputs.push(output_text(&done.output));
                    self.record_tool(live, done, &mut latched);
                }
            }
        }
        Ok(ToolPass {
            denied,
            latched,
            outputs,
        })
    }

    fn record_tool(&self, live: &mut Live, done: Done, latched: &mut Option<Decision>) {
        let event = if done.is_error {
            Event::ToolError {
                tool: done.name.clone(),
                error_code: done.error_code.clone(),
            }
        } else {
            Event::ToolOk {
                tool: done.name.clone(),
            }
        };
        self.trace.write_normal(
            "harness.tool",
            json!({
                "run_id": &live.row.run_id,
                "turn": live.row.turn_index,
                "tool": &done.name,
                "effect": effect_label(done.effect),
                "is_error": done.is_error,
                "error_code": &done.error_code,
            }),
        );
        live.transcript.push(TranscriptItem::Tool {
            call_id: done.call_id,
            name: done.name,
            output: done.output,
            is_error: done.is_error,
        });
        let decision = live.engine.feed(event);
        if latched.is_none() && !decision.is_continue() {
            *latched = Some(decision);
        }
    }

    fn dispatch(
        &self,
        launch: &Launch<'_>,
        live: &mut Live,
        index: usize,
        call: &crate::endpoint::ParsedToolCall,
    ) -> Result<Dispatch> {
        match live.cred.authorize(&call.name) {
            Ok(()) => {}
            Err(Error::Unauthorized {
                tool,
                fact_present,
                code,
                detail,
            }) => {
                return Ok(Dispatch::Denied(RunStatus::Unauthorized {
                    tool,
                    fact_present,
                    code,
                    detail,
                }));
            }
            Err(err) => return Err(err),
        }
        let Some(decl) = launch.spec.tool(&call.name) else {
            return Ok(Dispatch::Denied(RunStatus::Unauthorized {
                tool: call.name.clone(),
                fact_present: false,
                code: crate::authz::AuthzCode::OutsideIntersection,
                detail: "规格里没有这个工具".into(),
            }));
        };
        if let Err(failure) = check_schema(decl, &call.arguments) {
            return Ok(Dispatch::Done(done_error(call, failure, decl.effect)));
        }
        if decl.effect.is_side_effect() {
            self.side_effect(launch, live, index, call, decl.effect)
        } else {
            Ok(local_tool(launch, call, decl.effect))
        }
    }

    fn side_effect(
        &self,
        launch: &Launch<'_>,
        live: &mut Live,
        index: usize,
        call: &crate::endpoint::ParsedToolCall,
        effect: EffectClass,
    ) -> Result<Dispatch> {
        let offset = i64::try_from(index).unwrap_or(i64::MAX);
        if offset >= 1000 {
            return Err(Error::Protocol("单轮工具调用超过 1000".into()));
        }
        let key = IntentKey {
            workorder_id: launch.order.id().to_string(),
            step_path: format!("{}/{}", live.row.run_id, call.name),
            seq: i64::from(live.row.turn_index) * 1000 + offset,
        };
        let payload = json!({
            "arguments": call.arguments,
            "tool": call.name,
        });
        self.gateway
            .declare_intent(&self.session, &key, &payload, launch.order)
            .map_err(box_gateway)?;
        let registry = &launch.tools;
        let tool_name = call.name.clone();
        let outcome = self
            .gateway
            .execute(&self.session, &key, launch.order, move |stored| {
                let args = stored
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match registry.invoke(&tool_name, &args) {
                    Ok(output) => Ok(EffectReceipt { output }),
                    Err(failure) => Err(GatewayError::ExternalCall(
                        serde_json::to_string(&failure).unwrap_or_else(|_| "E_EXTERNAL".into()),
                    )),
                }
            });
        match outcome {
            Ok(outcome) => {
                let replay = outcome.is_replay();
                let receipt = outcome.into_inner();
                if replay {
                    live.row.effect_replays = live.row.effect_replays.saturating_add(1);
                } else {
                    live.row.effect_fresh = live.row.effect_fresh.saturating_add(1);
                }
                Ok(Dispatch::Done(done_ok(call, receipt.output, effect)))
            }
            Err(GatewayError::ExternalCall(message)) => {
                let failure = serde_json::from_str(&message).unwrap_or(ToolFailure {
                    error_code: "E_EXTERNAL".into(),
                    message,
                });
                Ok(Dispatch::Done(done_error(call, failure, effect)))
            }
            Err(err) => Err(box_gateway(err)),
        }
    }

    fn finish(&self, live: &mut Live, status: RunStatus) -> Result<RunReport> {
        self.trace_terminal(&live.row, &status)?;
        live.row.terminal_json = Some(serde_json::to_string(&status)?);
        self.persist(live)?;
        report_of(&live.row, status)
    }

    fn persist(&self, live: &mut Live) -> Result<()> {
        live.row.transcript_json = serde_json::to_string(&live.transcript)?;
        live.row.stop_state_json = serde_json::to_string(&live.engine.snapshot())?;
        let mut record: BudgetRecord = serde_json::from_str(&live.row.budget_json)?;
        record.write_current(&live.budget)?;
        live.row.budget_json = serde_json::to_string(&record)?;
        live.row.updated_at_ms = self.clock.wall_ms();
        self.store.update_runtime(&live.row)?;
        Ok(())
    }

    fn trace_terminal(&self, row: &RunRecord, status: &RunStatus) -> Result<()> {
        let kind = match status {
            RunStatus::BudgetExhausted { .. } => "harness.budget",
            RunStatus::Unauthorized { .. } => "harness.unauthorized",
            _ => "harness.stop",
        };
        self.trace
            .write_critical(
                kind,
                json!({
                    "run_id": row.run_id,
                    "turn": row.turn_index,
                    "status": status,
                }),
            )
            .map_err(|err| Error::Trace(err.to_string()))?;
        Ok(())
    }

    fn trace_stop(&self, row: &RunRecord, decision: &Decision) -> Result<()> {
        self.trace
            .write_critical(
                "harness.stop",
                json!({
                    "run_id": row.run_id,
                    "turn": row.turn_index,
                    "decision": decision,
                }),
            )
            .map_err(|err| Error::Trace(err.to_string()))?;
        Ok(())
    }
}

fn fresh_row(launch: &Launch<'_>, cred: &RunCredential, now_ms: u64) -> Result<RunRecord> {
    let budget = BudgetRecord::fresh(&launch.budget)?;
    let engine = StopEngine::new(launch.stop_policy.clone());
    let transcript = vec![TranscriptItem::User {
        text: launch.user_message.clone(),
    }];
    Ok(RunRecord {
        run_id: launch.run_id.clone(),
        spec_json: serde_json::to_string(&launch.spec)?,
        vars_json: serde_json::to_string(&launch.vars)?,
        user_message: launch.user_message.clone(),
        namespace: launch.namespace.clone(),
        spend_cap_cents: launch.spend_cap_cents,
        tool_spend_cents: launch.tool_spend_cents,
        order_id: launch.order.id().to_string(),
        stop_policy_json: serde_json::to_string(&launch.stop_policy)?,
        budget_json: serde_json::to_string(&budget)?,
        turn_index: 0,
        tool_free_streak: 0,
        transcript_json: serde_json::to_string(&transcript)?,
        stop_state_json: serde_json::to_string(&engine.snapshot())?,
        model_calls: 0,
        effect_fresh: 0,
        effect_replays: 0,
        terminal_json: None,
        whitelist_json: serde_json::to_string(&cred.tools)?,
        updated_at_ms: now_ms,
    })
}

fn local_tool(
    launch: &Launch<'_>,
    call: &crate::endpoint::ParsedToolCall,
    effect: EffectClass,
) -> Dispatch {
    match launch.tools.invoke(&call.name, &call.arguments) {
        Ok(output) => Dispatch::Done(done_ok(call, output, effect)),
        Err(failure) => Dispatch::Done(done_error(call, failure, effect)),
    }
}

fn done_ok(call: &crate::endpoint::ParsedToolCall, output: Value, effect: EffectClass) -> Done {
    Done {
        call_id: call.id.clone(),
        name: call.name.clone(),
        output,
        is_error: false,
        error_code: String::new(),
        effect,
    }
}

fn done_error(
    call: &crate::endpoint::ParsedToolCall,
    failure: ToolFailure,
    effect: EffectClass,
) -> Done {
    Done {
        call_id: call.id.clone(),
        name: call.name.clone(),
        output: json!({
            "error_code": failure.error_code,
            "message": failure.message,
        }),
        is_error: true,
        error_code: failure.error_code,
        effect,
    }
}

fn check_schema(tool: &ToolDecl, args: &Value) -> std::result::Result<(), ToolFailure> {
    let Some(required) = tool.parameters.get("required").and_then(Value::as_array) else {
        return Ok(());
    };
    let Some(object) = args.as_object() else {
        return Err(ToolFailure {
            error_code: "E_SCHEMA".into(),
            message: format!("工具 {} 的参数必须是对象", tool.name),
        });
    };
    for item in required {
        let Some(name) = item.as_str() else {
            continue;
        };
        if !object.contains_key(name) {
            return Err(ToolFailure {
                error_code: "E_SCHEMA".into(),
                message: format!("工具 {} 缺少参数 {name}", tool.name),
            });
        }
    }
    Ok(())
}

fn visible_tools(spec: &AgentSpec, whitelist: &[String]) -> Vec<ToolDecl> {
    spec.toolset
        .iter()
        .filter(|tool| whitelist.iter().any(|name| name == &tool.name))
        .cloned()
        .collect()
}

/// fix2 收尾机制阈值：剩 total/factor 轮（下限 floor）触发提醒。
fn turn_reminder_threshold(total: u32, factor: u32, floor: u32) -> u32 {
    (total / factor).max(floor).min(total)
}

/// fix2 完成信号匹配：任一工具输出文本的**首行**（trim 后）完整等于某信号词才算。
/// 因果、可复核（完成必付出一个真实执行的动作），叙述性提及不触发。
fn matched_signal(outputs: &[String], signals: &[String]) -> Option<String> {
    signals
        .iter()
        .find(|signal| {
            outputs.iter().any(|out| {
                out.lines()
                    .next()
                    .map(|line| line.trim() == signal.as_str())
                    == Some(true)
            })
        })
        .cloned()
}

fn terminal_status(row: &RunRecord) -> Result<Option<RunStatus>> {
    match &row.terminal_json {
        None => Ok(None),
        Some(text) => Ok(Some(parse_stored(text, "终态")?)),
    }
}

fn report_of(row: &RunRecord, status: RunStatus) -> Result<RunReport> {
    let transcript: Vec<TranscriptItem> = parse_stored(&row.transcript_json, "对话快照")?;
    let mut tokens = TokenTotals::default();
    for item in &transcript {
        if let TranscriptItem::Assistant { usage: Some(u), .. } = item {
            tokens.input = tokens.input.saturating_add(u.input_tokens);
            tokens.output = tokens.output.saturating_add(u.output_tokens);
            tokens.cache_read = tokens.cache_read.saturating_add(u.cache_read_tokens);
        }
    }
    Ok(RunReport {
        run_id: row.run_id.clone(),
        status,
        turns: row.turn_index,
        model_calls: row.model_calls,
        effect_replays: row.effect_replays,
        effect_fresh: row.effect_fresh,
        transcript,
        whitelist: parse_stored(&row.whitelist_json, "白名单")?,
        tokens,
    })
}

fn parse_stored<T: for<'de> Deserialize<'de>>(text: &str, what: &str) -> Result<T> {
    serde_json::from_str(text)
        .map_err(|err| Error::DeclarationMismatch(format!("{what} 无法读取: {err}")))
}

fn mismatch(text: &str) -> Error {
    Error::DeclarationMismatch(text.into())
}

fn box_gateway(err: GatewayError) -> Error {
    Error::Gateway(Box::new(err))
}
