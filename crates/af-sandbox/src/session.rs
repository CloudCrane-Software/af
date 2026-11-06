//! 会话句柄与纪元围栏。
//!
//! 参照 af-effect-gateway：宿主持有当前纪元，句柄携带签发时的纪元。
//! [`SandboxHost::resume`] 把纪元加一。后端 [`crate::SandboxBackend::generation`] 变化
//! 同样把纪元加一。之后旧句柄上的 exec / collect / close 都是 [`SandboxError::StaleEpoch`]，
//! 不会去后端探活，也不会把旧 id 重新打开。
//!
//! `boot_id` 由调用方按进程注入。新进程若复用纪元计数器 0，只要 boot 不同，旧句柄仍然失败。
//! boot 与纪元都能对上、但本宿主没有这个 id 时返回 [`SandboxError::SessionGone`]，不向后端认领。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::backend::{BackendExec, BackendFailure, OpenGrant, SandboxBackend};
use crate::budget::{accept_charge, BudgetDecision, ConcurrencyCharge};
use crate::clock::Clock;
use crate::error::{Result, SandboxError};
use crate::scope::{allow_sandbox_exec, CredentialScope};
use crate::types::{Artifact, ArtifactSpec, Command, Image, Resources};

/// 可交给另一进程保存的句柄。它本身不是存活证明。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionHandle {
    pub id: u64,
    pub epoch: u64,
    pub boot_id: u64,
}

/// 一次 exec 的结果。非零退出码仍是 `Ok`：命令跑完了。超时和后端不可用不是 `Ok`。
///
/// `charge` 是调用方交来、且被原样接受的那份并发决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed_ms: u64,
    pub charge: ConcurrencyCharge,
}

struct LiveSession {
    handle: SessionHandle,
    opened_at_ms: u64,
    resources: Resources,
    image: Image,
    grant: OpenGrant,
    closed: bool,
}

struct HostState {
    epoch: u64,
    seen_generation: u64,
    next_id: u64,
    sessions: BTreeMap<u64, LiveSession>,
    backend: Box<dyn SandboxBackend>,
}

struct HostShared {
    boot_id: u64,
    clock: Arc<dyn Clock>,
    state: Mutex<HostState>,
}

/// 沙箱宿主。会话方法通过它做围栏和后端调用。
#[derive(Clone)]
pub struct SandboxHost {
    shared: Arc<HostShared>,
}

impl std::fmt::Debug for SandboxHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SandboxHost")
            .field("boot_id", &self.shared.boot_id)
            .finish()
    }
}

impl SandboxHost {
    /// `boot_id` 每个进程一份，由调用方注入。本函数不取随机数，也不读时钟。
    pub fn boot(
        backend: impl SandboxBackend + 'static,
        clock: impl Clock + 'static,
        boot_id: u64,
    ) -> Self {
        let seen_generation = backend.generation();
        Self {
            shared: Arc::new(HostShared {
                boot_id,
                clock: Arc::new(clock),
                state: Mutex::new(HostState {
                    epoch: 0,
                    seen_generation,
                    next_id: 1,
                    sessions: BTreeMap::new(),
                    backend: Box::new(backend),
                }),
            }),
        }
    }

    pub fn boot_id(&self) -> u64 {
        self.shared.boot_id
    }

    /// 观察后端代际后的当前纪元。代际变了会先把纪元加一。
    pub fn current_epoch(&self) -> Result<u64> {
        let mut state = self.shared.lock()?;
        sync_generation(&mut state)?;
        Ok(state.epoch)
    }

    /// 接管或进程内重启。纪元加一，本地会话作废，并要求后端丢掉全部会话。
    ///
    /// 后端丢掉会话失败时，纪元仍然已经前进，返回 [`SandboxError::SandboxDown`]。
    /// 旧句柄不能再执行。
    pub fn resume(&self) -> Result<u64> {
        let mut state = self.shared.lock()?;
        sync_generation(&mut state)?;
        state.epoch = state
            .epoch
            .checked_add(1)
            .ok_or(SandboxError::CounterExhausted)?;
        for session in state.sessions.values_mut() {
            session.closed = true;
        }
        let epoch = state.epoch;
        state.backend.retire_all().map_err(map_failure)?;
        Ok(epoch)
    }
}

/// 绑定在某个宿主上的会话。克隆共享同一份宿主状态。
#[derive(Clone)]
pub struct Session {
    handle: SessionHandle,
    host: Arc<HostShared>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("handle", &self.handle)
            .finish()
    }
}

impl Session {
    /// 打开会话。资源限额原样交给后端。不检查凭证，也不扣预算。
    pub fn open(host: &SandboxHost, image: Image, resources: Resources) -> Result<Self> {
        let opened_at_ms = host.shared.clock.now_ms();
        let mut state = host.shared.lock()?;
        sync_generation(&mut state)?;
        if !state.backend.available() {
            return Err(SandboxError::SandboxDown);
        }
        let id = state.next_id;
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or(SandboxError::CounterExhausted)?;
        if state.sessions.contains_key(&id) {
            return Err(SandboxError::BackendRejected {
                detail: "session id collision".to_string(),
            });
        }
        let grant = state
            .backend
            .open(&image, &resources)
            .map_err(map_failure)?;
        let handle = SessionHandle {
            id,
            epoch: state.epoch,
            boot_id: host.shared.boot_id,
        };
        state.sessions.insert(
            id,
            LiveSession {
                handle,
                opened_at_ms,
                resources,
                image,
                grant,
                closed: false,
            },
        );
        Ok(Self {
            handle,
            host: Arc::clone(&host.shared),
        })
    }

    /// 用保存的句柄重新绑定。不向后端查询这个 id 是否还在跑。
    pub fn reattach(host: &SandboxHost, handle: SessionHandle) -> Result<Self> {
        let mut state = host.shared.lock()?;
        sync_generation(&mut state)?;
        require_live(&host.shared, &state, handle)?;
        Ok(Self {
            handle,
            host: Arc::clone(&host.shared),
        })
    }

    pub fn handle(&self) -> SessionHandle {
        self.handle
    }

    /// 句柄上的纪元，不是宿主当前纪元。
    pub fn epoch(&self) -> u64 {
        self.handle.epoch
    }

    pub fn id(&self) -> u64 {
        self.handle.id
    }

    /// 打开时注入的镜像。句柄失效时不返回，避免把失效会话说成还拿着这份配置在跑。
    pub fn image(&self) -> Result<Image> {
        let view = self.view()?;
        Ok(view.image)
    }

    /// 打开时注入的限额。
    pub fn resources(&self) -> Result<Resources> {
        let view = self.view()?;
        Ok(view.resources)
    }

    /// 打开时的注入时钟读数。
    pub fn opened_at_ms(&self) -> Result<u64> {
        let view = self.view()?;
        Ok(view.opened_at_ms)
    }

    /// 在沙箱里执行。
    ///
    /// 顺序：围栏、后端可用性、会话超时、凭证白名单、预算决策、后端、耗时上限。
    /// 凭证与预算失败都不会调用后端。`budget` 必须是调用方事先做好的决策。
    pub fn exec(
        &self,
        cmd: &Command,
        credential_scope: &CredentialScope<'_>,
        budget: &BudgetDecision,
    ) -> Result<ExecOutput> {
        let now_ms = self.host.clock.now_ms();
        let mut state = self.host.lock()?;
        sync_generation(&mut state)?;
        let view = require_live(&self.host, &state, self.handle)?;
        if !state.backend.available() {
            return Err(SandboxError::SandboxDown);
        }
        if past_deadline(view.opened_at_ms, view.resources.timeout_ms(), now_ms) {
            return Err(SandboxError::TimedOut {
                timeout_ms: view.resources.timeout_ms(),
                elapsed_ms: now_ms.saturating_sub(view.opened_at_ms),
            });
        }
        allow_sandbox_exec(credential_scope)?;
        let charge = accept_charge(budget)?;
        let grant = view.grant.clone();
        let resources = view.resources;
        let outcome = state
            .backend
            .exec(&grant, cmd, &resources)
            .map_err(map_failure)?;
        if outcome.elapsed_ms >= resources.timeout_ms() {
            return Err(SandboxError::TimedOut {
                timeout_ms: resources.timeout_ms(),
                elapsed_ms: outcome.elapsed_ms,
            });
        }
        Ok(output_from(outcome, charge))
    }

    /// 按路径取产物。缺一个路径则整次失败，不返回已经读到的其它字节。
    ///
    /// 不检查凭证和预算。会话超时后仍可取，以便调用方收走超时前写出的文件。
    pub fn collect(&self, artifacts: &[ArtifactSpec]) -> Result<Vec<Artifact>> {
        let mut state = self.host.lock()?;
        sync_generation(&mut state)?;
        let view = require_live(&self.host, &state, self.handle)?;
        if !state.backend.available() {
            return Err(SandboxError::SandboxDown);
        }
        let grant = view.grant.clone();
        state
            .backend
            .collect(&grant, artifacts)
            .map_err(map_failure)
    }

    /// 关闭会话。同一纪元里的重复关闭是成功，且不再次调用后端。
    ///
    /// 旧纪元返回 [`SandboxError::StaleEpoch`]，不把关闭说成成功。
    /// 后端不可用时会话保持打开，调用方可在后端恢复后重试。
    pub fn close(&self) -> Result<()> {
        let mut state = self.host.lock()?;
        sync_generation(&mut state)?;
        match classify_close(&self.host, &state, self.handle)? {
            CloseTarget::AlreadyClosed => Ok(()),
            CloseTarget::Open(_) if !state.backend.available() => Err(SandboxError::SandboxDown),
            CloseTarget::Open(grant) => {
                state.backend.close(&grant).map_err(map_failure)?;
                if let Some(session) = state.sessions.get_mut(&self.handle.id) {
                    session.closed = true;
                }
                Ok(())
            }
        }
    }

    fn view(&self) -> Result<LiveView> {
        let mut state = self.host.lock()?;
        sync_generation(&mut state)?;
        require_live(&self.host, &state, self.handle)
    }
}

struct LiveView {
    opened_at_ms: u64,
    resources: Resources,
    image: Image,
    grant: OpenGrant,
}

enum CloseTarget {
    AlreadyClosed,
    Open(OpenGrant),
}

impl HostShared {
    fn lock(&self) -> Result<MutexGuard<'_, HostState>> {
        self.state.lock().map_err(|_| SandboxError::HostPoisoned)
    }
}

fn sync_generation(state: &mut HostState) -> Result<()> {
    let generation = state.backend.generation();
    if generation != state.seen_generation {
        state.seen_generation = generation;
        state.epoch = state
            .epoch
            .checked_add(1)
            .ok_or(SandboxError::CounterExhausted)?;
    }
    Ok(())
}

fn require_live(shared: &HostShared, state: &HostState, handle: SessionHandle) -> Result<LiveView> {
    if handle.boot_id != shared.boot_id || handle.epoch != state.epoch {
        return Err(stale(shared, state, handle));
    }
    match state.sessions.get(&handle.id) {
        None => Err(SandboxError::SessionGone {
            session_id: handle.id,
        }),
        Some(session) if session.handle != handle => Err(stale(shared, state, handle)),
        Some(session) if session.closed => Err(SandboxError::SessionClosed {
            session_id: handle.id,
        }),
        Some(session) => Ok(LiveView {
            opened_at_ms: session.opened_at_ms,
            resources: session.resources,
            image: session.image.clone(),
            grant: session.grant.clone(),
        }),
    }
}

fn classify_close(
    shared: &HostShared,
    state: &HostState,
    handle: SessionHandle,
) -> Result<CloseTarget> {
    if handle.boot_id != shared.boot_id || handle.epoch != state.epoch {
        return Err(stale(shared, state, handle));
    }
    match state.sessions.get(&handle.id) {
        None => Err(SandboxError::SessionGone {
            session_id: handle.id,
        }),
        Some(session) if session.handle != handle => Err(stale(shared, state, handle)),
        Some(session) if session.closed => Ok(CloseTarget::AlreadyClosed),
        Some(session) => Ok(CloseTarget::Open(session.grant.clone())),
    }
}

fn stale(shared: &HostShared, state: &HostState, handle: SessionHandle) -> SandboxError {
    SandboxError::StaleEpoch {
        request_epoch: handle.epoch,
        current_epoch: state.epoch,
        request_boot: handle.boot_id,
        current_boot: shared.boot_id,
    }
}

fn map_failure(failure: BackendFailure) -> SandboxError {
    match failure {
        BackendFailure::Down => SandboxError::SandboxDown,
        BackendFailure::MissingArtifact { path } => SandboxError::ArtifactMissing { path },
        BackendFailure::Rejected { detail } => SandboxError::BackendRejected { detail },
    }
}

fn output_from(outcome: BackendExec, charge: ConcurrencyCharge) -> ExecOutput {
    ExecOutput {
        exit_code: outcome.exit_code,
        stdout: outcome.stdout,
        stderr: outcome.stderr,
        elapsed_ms: outcome.elapsed_ms,
        charge,
    }
}

/// 到达上限即超时。加法溢出无法表示截止点，按超时拒绝。
fn past_deadline(opened_at_ms: u64, timeout_ms: u64, now_ms: u64) -> bool {
    match opened_at_ms.checked_add(timeout_ms) {
        Some(deadline) => now_ms >= deadline,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::past_deadline;

    #[test]
    fn deadline_is_inclusive_and_overflow_fails_closed() {
        assert!(!past_deadline(1_000, 5_000, 5_999));
        assert!(past_deadline(1_000, 5_000, 6_000));
        assert!(past_deadline(u64::MAX, 1, u64::MAX));
    }
}
