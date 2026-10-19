//! 沙箱后端。本 crate 的语义层只依赖这个 trait。
//!
//! [`StubHandle`] 是进程内的 mock，给测试和没有真实沙箱的调用方用。
//! CubeSandbox 的线路协议不在这里。真实集成依赖本机环境，不进 `cargo test`。

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use crate::types::{Artifact, ArtifactSpec, Command, Image, Resources};

/// 后端打开会话后交回的句柄。客户端把它存进纪元会话，不把它当成跨重启的存活证明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGrant {
    pub backend_session_id: u64,
}

/// 后端一次执行的报告。超时由客户端用 `elapsed_ms` 和注入的限额判定，后端不睡眠。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendExec {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed_ms: u64,
}

/// 后端失败。不可用与“这次请求被拒绝”分开。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendFailure {
    Down,
    MissingArtifact { path: String },
    Rejected { detail: String },
}

/// 执行面。实现必须在 `generation` 变化时丢掉旧会话，而不是继续认旧 id。
pub trait SandboxBackend: Send {
    fn available(&self) -> bool;

    /// 后端进程代。重启后必须变化。客户端用它作废已签发的会话。
    fn generation(&self) -> u64;

    fn open(&mut self, image: &Image, resources: &Resources) -> Result<OpenGrant, BackendFailure>;

    fn exec(
        &mut self,
        grant: &OpenGrant,
        cmd: &Command,
        resources: &Resources,
    ) -> Result<BackendExec, BackendFailure>;

    fn collect(
        &mut self,
        grant: &OpenGrant,
        artifacts: &[ArtifactSpec],
    ) -> Result<Vec<Artifact>, BackendFailure>;

    fn close(&mut self, grant: &OpenGrant) -> Result<(), BackendFailure>;

    /// 丢掉本后端上的全部会话。客户端 `resume` 时调用。失败不得被当成“已经没有旧会话”。
    fn retire_all(&mut self) -> Result<(), BackendFailure>;
}

/// 记录一次 open 实际收到的限额，用来断言参数被原样注入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRecord {
    pub image: Image,
    pub resources: Resources,
}

/// 记录一次 exec 实际收到的命令和限额。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecRecord {
    pub backend_session_id: u64,
    pub argv: Vec<String>,
    pub resources: Resources,
}

#[derive(Debug)]
struct StubState {
    available: bool,
    generation: u64,
    next_id: u64,
    sessions: BTreeMap<u64, ()>,
    execs: BTreeMap<Vec<String>, BackendExec>,
    artifacts: BTreeMap<String, Vec<u8>>,
    opens: Vec<OpenRecord>,
    exec_log: Vec<ExecRecord>,
    exec_count: u64,
    collect_count: u64,
    close_count: u64,
    retire_count: u64,
}

impl StubState {
    fn new() -> Self {
        Self {
            available: true,
            generation: 1,
            next_id: 1,
            sessions: BTreeMap::new(),
            execs: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            opens: Vec::new(),
            exec_log: Vec::new(),
            exec_count: 0,
            collect_count: 0,
            close_count: 0,
            retire_count: 0,
        }
    }
}

/// 共享的 stub 后端。克隆指向同一份状态，便于测试在宿主之外观察。
#[derive(Debug, Clone)]
pub struct StubHandle {
    inner: std::sync::Arc<Mutex<StubState>>,
}

impl StubHandle {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(StubState::new())),
        }
    }

    pub fn set_available(&self, available: bool) {
        if let Ok(mut state) = self.inner.lock() {
            state.available = available;
        }
    }

    /// 模拟后端进程重启：代际 +1，已打开的后端会话消失。脚本和产物表保留。
    pub fn restart(&self) {
        if let Ok(mut state) = self.inner.lock() {
            match state.generation.checked_add(1) {
                Some(next) => {
                    state.generation = next;
                    state.sessions.clear();
                }
                None => state.available = false,
            }
        }
    }

    pub fn script_exec(&self, argv: &[String], outcome: BackendExec) {
        if let Ok(mut state) = self.inner.lock() {
            state.execs.insert(argv.to_vec(), outcome);
        }
    }

    pub fn insert_artifact(&self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        if let Ok(mut state) = self.inner.lock() {
            state.artifacts.insert(path.into(), bytes.into());
        }
    }

    pub fn open_log(&self) -> Vec<OpenRecord> {
        self.inner
            .lock()
            .map(|state| state.opens.clone())
            .unwrap_or_default()
    }

    pub fn exec_log(&self) -> Vec<ExecRecord> {
        self.inner
            .lock()
            .map(|state| state.exec_log.clone())
            .unwrap_or_default()
    }

    pub fn exec_count(&self) -> u64 {
        self.inner.lock().map(|state| state.exec_count).unwrap_or(0)
    }

    pub fn collect_count(&self) -> u64 {
        self.inner
            .lock()
            .map(|state| state.collect_count)
            .unwrap_or(0)
    }

    pub fn close_count(&self) -> u64 {
        self.inner
            .lock()
            .map(|state| state.close_count)
            .unwrap_or(0)
    }

    pub fn retire_count(&self) -> u64 {
        self.inner
            .lock()
            .map(|state| state.retire_count)
            .unwrap_or(0)
    }

    pub fn session_count(&self) -> usize {
        self.inner
            .lock()
            .map(|state| state.sessions.len())
            .unwrap_or(0)
    }

    fn lock(&self) -> Result<MutexGuard<'_, StubState>, BackendFailure> {
        self.inner.lock().map_err(|_| BackendFailure::Rejected {
            detail: "stub lock poisoned".to_string(),
        })
    }
}

impl Default for StubHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxBackend for StubHandle {
    fn available(&self) -> bool {
        self.inner
            .lock()
            .map(|state| state.available)
            .unwrap_or(false)
    }

    fn generation(&self) -> u64 {
        self.inner.lock().map(|state| state.generation).unwrap_or(0)
    }

    fn open(&mut self, image: &Image, resources: &Resources) -> Result<OpenGrant, BackendFailure> {
        let mut state = self.lock()?;
        if !state.available {
            return Err(BackendFailure::Down);
        }
        let backend_session_id = state.next_id;
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or(BackendFailure::Rejected {
                detail: "stub session id exhausted".to_string(),
            })?;
        state.sessions.insert(backend_session_id, ());
        state.opens.push(OpenRecord {
            image: image.clone(),
            resources: *resources,
        });
        Ok(OpenGrant { backend_session_id })
    }

    fn exec(
        &mut self,
        grant: &OpenGrant,
        cmd: &Command,
        resources: &Resources,
    ) -> Result<BackendExec, BackendFailure> {
        let mut state = self.lock()?;
        if !state.available {
            return Err(BackendFailure::Down);
        }
        if !state.sessions.contains_key(&grant.backend_session_id) {
            return Err(BackendFailure::Rejected {
                detail: format!("unknown backend session {}", grant.backend_session_id),
            });
        }
        state.exec_count = state.exec_count.saturating_add(1);
        state.exec_log.push(ExecRecord {
            backend_session_id: grant.backend_session_id,
            argv: cmd.argv().to_vec(),
            resources: *resources,
        });
        let outcome = state.execs.get(cmd.argv()).cloned().unwrap_or(BackendExec {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            elapsed_ms: 0,
        });
        Ok(outcome)
    }

    fn collect(
        &mut self,
        grant: &OpenGrant,
        artifacts: &[ArtifactSpec],
    ) -> Result<Vec<Artifact>, BackendFailure> {
        let mut state = self.lock()?;
        if !state.available {
            return Err(BackendFailure::Down);
        }
        if !state.sessions.contains_key(&grant.backend_session_id) {
            return Err(BackendFailure::Rejected {
                detail: format!("unknown backend session {}", grant.backend_session_id),
            });
        }
        state.collect_count = state.collect_count.saturating_add(1);
        let mut out = Vec::with_capacity(artifacts.len());
        for spec in artifacts {
            match state.artifacts.get(spec.path()) {
                Some(bytes) => out.push(Artifact {
                    path: spec.path().to_string(),
                    bytes: bytes.clone(),
                }),
                None => {
                    return Err(BackendFailure::MissingArtifact {
                        path: spec.path().to_string(),
                    });
                }
            }
        }
        Ok(out)
    }

    fn close(&mut self, grant: &OpenGrant) -> Result<(), BackendFailure> {
        let mut state = self.lock()?;
        if !state.available {
            return Err(BackendFailure::Down);
        }
        if state.sessions.remove(&grant.backend_session_id).is_none() {
            return Err(BackendFailure::Rejected {
                detail: format!("unknown backend session {}", grant.backend_session_id),
            });
        }
        state.close_count = state.close_count.saturating_add(1);
        Ok(())
    }

    fn retire_all(&mut self) -> Result<(), BackendFailure> {
        let mut state = self.lock()?;
        if !state.available {
            return Err(BackendFailure::Down);
        }
        state.sessions.clear();
        state.retire_count = state.retire_count.saturating_add(1);
        Ok(())
    }
}
