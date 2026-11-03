//! 门套件与 `af.gate_result.v1`。
//!
//! 门套件不是第二套评测账本：每个检查定义就是既有 [`BenchCase`] 的 JSON
//! payload，freeze/fork/盲评/污染停机全部复用 [`Catalog`]。门结果只引用账本中的
//! `ledger_id/run_id` 与冻结快照，不把当前配方带进运行视图。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{EvalError, Result as EvalResult};
use crate::hash::{sha256_hex, SHA256_HEX_LEN};
use crate::{Benchmark, Catalog, RunVerdict};

pub const GATE_RESULT_TYPE: &str = "af.gate_result.v1";
pub const GATE_RESULT_PORT: &str = "gate_result";

/// 门检查定义。`input` 是交给 runner 的冻结输入，不是运行时recipe。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateCheckDefinition {
    pub check_id: String,
    pub required: bool,
    pub input: serde_json::Value,
}

/// 新建并冻结门套件 v1。内容改动必须走 [`fork_gate_suite`]。
pub fn freeze_gate_suite(
    catalog: &mut Catalog,
    lineage_id: &str,
    checks: &[GateCheckDefinition],
) -> EvalResult<Benchmark> {
    validate_definitions(checks)?;
    catalog.create_benchmark(lineage_id)?;
    for check in checks {
        catalog.put_case(lineage_id, 1, &check.check_id, &definition_payload(check)?)?;
    }
    catalog.freeze(lineage_id, 1)
}

/// 从已冻结的最新版 fork 下一版并冻结；父谱系钉由 Catalog 原样保留。
pub fn fork_gate_suite(
    catalog: &mut Catalog,
    lineage_id: &str,
    version: u64,
    checks: &[GateCheckDefinition],
) -> EvalResult<Benchmark> {
    validate_definitions(checks)?;
    let forked = catalog.fork(lineage_id, version)?;
    let wanted: BTreeSet<&str> = checks.iter().map(|c| c.check_id.as_str()).collect();
    for old in &forked.cases {
        if !wanted.contains(old.case_id.as_str()) {
            catalog.remove_case(lineage_id, forked.version, &old.case_id)?;
        }
    }
    for check in checks {
        catalog.put_case(
            lineage_id,
            forked.version,
            &check.check_id,
            &definition_payload(check)?,
        )?;
    }
    catalog.freeze(lineage_id, forked.version)
}

/// 从盲评 RunView 的冻结快照读门定义；任何漂移都按 E_BENCH 处理。
pub fn gate_definitions(view: &crate::RunView) -> EvalResult<Vec<GateCheckDefinition>> {
    view.snapshot
        .cases
        .iter()
        .map(|case| {
            let check: GateCheckDefinition =
                serde_json::from_str(&case.payload).map_err(|err| EvalError::Corrupt {
                    detail: format!("门定义 {} 不是合法 JSON: {err}", case.case_id),
                })?;
            if check.check_id != case.case_id {
                return Err(EvalError::Corrupt {
                    detail: format!(
                        "门定义 id 漂移: case_id={} payload.check_id={}",
                        case.case_id, check.check_id
                    ),
                });
            }
            Ok(check)
        })
        .collect()
}

fn definition_payload(check: &GateCheckDefinition) -> EvalResult<String> {
    serde_json::to_string(check).map_err(|err| EvalError::Corrupt {
        detail: format!("门定义序列化失败: {err}"),
    })
}

fn validate_definitions(checks: &[GateCheckDefinition]) -> EvalResult<()> {
    if checks.is_empty() {
        return Err(EvalError::EmptyBenchmark);
    }
    let mut ids = BTreeSet::new();
    for check in checks {
        if check.check_id.is_empty() || !ids.insert(check.check_id.as_str()) {
            return Err(EvalError::invalid("check_id"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateVerdict {
    Pass,
    Fail,
    NeedsHuman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateFailureCode {
    #[serde(rename = "E_INFRA")]
    Infra,
    #[serde(rename = "E_SUBJECT")]
    Subject,
    #[serde(rename = "E_BENCH")]
    Bench,
    #[serde(rename = "E_TIMEOUT")]
    Timeout,
}

/// SKIP 的三类原因分别入账；required SKIP 永不计绿。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// required 项不适用 = 门定义与被测对象不匹配（E_BENCH）。
    NotApplicable,
    /// 环境/依赖不可用（E_INFRA）。
    EnvironmentUnavailable,
    /// required 项被配方排除 = 门定义不可信（E_BENCH）。
    ExplicitlyExcluded,
}

impl SkipReason {
    fn failure_code(self) -> GateFailureCode {
        match self {
            Self::EnvironmentUnavailable => GateFailureCode::Infra,
            Self::NotApplicable | Self::ExplicitlyExcluded => GateFailureCode::Bench,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateCheckStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateCheckResult {
    pub check_id: String,
    pub required: bool,
    pub status: GateCheckStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<SkipReason>,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkipAccounting {
    pub not_applicable: u64,
    pub environment_unavailable: u64,
    pub explicitly_excluded: u64,
}

impl SkipAccounting {
    pub fn of(checks: &[GateCheckResult]) -> Self {
        let mut out = Self::default();
        for check in checks {
            if check.status != GateCheckStatus::Skip {
                continue;
            }
            let counter = match check.skip_reason {
                Some(SkipReason::NotApplicable) => &mut out.not_applicable,
                Some(SkipReason::EnvironmentUnavailable) => &mut out.environment_unavailable,
                Some(SkipReason::ExplicitlyExcluded) => &mut out.explicitly_excluded,
                None => continue,
            };
            *counter = counter.saturating_add(1);
        }
        out
    }

    pub fn total(&self) -> u64 {
        self.not_applicable
            .saturating_add(self.environment_unavailable)
            .saturating_add(self.explicitly_excluded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvalRef {
    pub ledger_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSuiteRef {
    pub lineage_id: String,
    pub version: u64,
    pub snapshot_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateSubjectKind {
    GitCommit,
    WorkspaceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceFile {
    pub path: String,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSubject {
    pub kind: GateSubjectKind,
    pub repo: String,
    /// git_commit = 完整 40 位 commit；workspace_snapshot = files 的规范摘要。
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<WorkspaceFile>,
}

/// 前级工件引用。下游 admit 以 digest 取回同一工件，再检查 payload.verdict。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateInput {
    pub artifact_type: String,
    pub content_sha256: String,
    pub producer_order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateRunnerRef {
    pub r#impl: String,
    pub digest: String,
    pub environment_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvidence {
    pub name: String,
    pub uri: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateResult {
    pub producer_order_id: String,
    pub eval: GateEvalRef,
    pub suite: GateSuiteRef,
    pub subject: GateSubject,
    pub recipe_digest: String,
    pub runner: GateRunnerRef,
    pub inputs: Vec<GateInput>,
    pub checks: Vec<GateCheckResult>,
    pub skip_accounting: SkipAccounting,
    pub verdict: GateVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<GateFailureCode>,
    pub evidence: Vec<GateEvidence>,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
}

/// 与 Task Graph 相同的 envelope v1 外壳；payload 才是门结果本体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateResultEnvelope {
    pub envelope: String,
    pub artifact_type: String,
    pub from: String,
    pub port: String,
    pub payload: GateResult,
}

impl GateResultEnvelope {
    pub fn new(payload: GateResult) -> Self {
        Self {
            envelope: "v1".into(),
            artifact_type: GATE_RESULT_TYPE.into(),
            from: payload.producer_order_id.clone(),
            port: GATE_RESULT_PORT.into(),
            payload,
        }
    }

    pub fn validate(&self) -> std::result::Result<(), GateError> {
        if self.envelope != "v1"
            || self.artifact_type != GATE_RESULT_TYPE
            || self.port != GATE_RESULT_PORT
            || self.from != self.payload.producer_order_id
        {
            return Err(GateError::Invalid(
                "envelope v1 外壳或生产者绑定不符".into(),
            ));
        }
        self.payload.validate()
    }

    pub fn persistent_bytes(&self) -> std::result::Result<Vec<u8>, GateError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|err| GateError::Json(err.to_string()))
    }
}

impl GateResult {
    pub fn validate(&self) -> std::result::Result<(), GateError> {
        for value in [
            &self.producer_order_id,
            &self.eval.ledger_id,
            &self.eval.run_id,
            &self.suite.lineage_id,
            &self.subject.repo,
            &self.runner.r#impl,
        ] {
            if value.trim().is_empty() {
                return Err(GateError::Invalid("必填标识为空".into()));
            }
        }
        for digest in [
            &self.suite.snapshot_hash,
            &self.recipe_digest,
            &self.runner.digest,
            &self.runner.environment_digest,
        ] {
            require_digest(digest, "门工件摘要")?;
        }
        match self.subject.kind {
            GateSubjectKind::GitCommit => {
                if self.subject.version.len() != 40 || !is_lower_hex(&self.subject.version) {
                    return Err(GateError::Invalid(
                        "subject.kind=git_commit 时 version 必须是完整 40 位 commit".into(),
                    ));
                }
                if !self.subject.files.is_empty() {
                    return Err(GateError::Invalid(
                        "git_commit 不得携带 workspace_snapshot 文件清单".into(),
                    ));
                }
            }
            GateSubjectKind::WorkspaceSnapshot => {
                let computed = workspace_snapshot_digest(&self.subject.files)?;
                if computed != self.subject.version {
                    return Err(GateError::Invalid(
                        "workspace_snapshot version 与排序文件清单摘要不符".into(),
                    ));
                }
            }
        }
        for input in &self.inputs {
            if input.artifact_type.trim().is_empty() || input.producer_order_id.trim().is_empty() {
                return Err(GateError::Invalid("inputs[] 必填标识为空".into()));
            }
            require_digest(&input.content_sha256, "inputs[].content_sha256")?;
        }
        for evidence in &self.evidence {
            if evidence.name.trim().is_empty() || evidence.uri.trim().is_empty() {
                return Err(GateError::Invalid("evidence[] 必填标识为空".into()));
            }
            require_digest(&evidence.sha256, "evidence[].sha256")?;
        }
        if self.finished_at_ms < self.started_at_ms {
            return Err(GateError::Invalid(
                "finished_at_ms 早于 started_at_ms".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for check in &self.checks {
            if check.check_id.trim().is_empty() || !ids.insert(&check.check_id) {
                return Err(GateError::Invalid("checks[] check_id 为空或重复".into()));
            }
            match (check.status, check.skip_reason) {
                (GateCheckStatus::Skip, Some(_)) => {}
                (GateCheckStatus::Skip, None) => {
                    return Err(GateError::Invalid("SKIP 必须声明三类原因之一".into()));
                }
                (_, Some(_)) => {
                    return Err(GateError::Invalid("非 SKIP 不得携带 skip_reason".into()));
                }
                (_, None) => {}
            }
        }
        if self.skip_accounting != SkipAccounting::of(&self.checks) {
            return Err(GateError::Invalid("SKIP 三类分账与 checks[] 不符".into()));
        }
        let expected = classify_checks(&self.checks);
        if self.verdict == GateVerdict::Pass
            && matches!(expected.0, GateVerdict::Fail | GateVerdict::NeedsHuman)
        {
            return Err(GateError::Invalid(
                "required FAIL/SKIP 不得序列化成 pass（防洗绿）".into(),
            ));
        }
        match self.verdict {
            GateVerdict::Pass if self.failure_code.is_some() => {
                return Err(GateError::Invalid("pass 不得携带 failure_code".into()));
            }
            GateVerdict::Fail if self.failure_code != Some(GateFailureCode::Subject) => {
                return Err(GateError::Invalid("fail 必须携带 E_SUBJECT".into()));
            }
            GateVerdict::NeedsHuman
                if !matches!(
                    self.failure_code,
                    Some(
                        GateFailureCode::Infra | GateFailureCode::Bench | GateFailureCode::Timeout
                    )
                ) =>
            {
                return Err(GateError::Invalid(
                    "needs_human 必须携带 E_INFRA/E_BENCH/E_TIMEOUT".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

/// 由账本终态与逐项检查生成严格三态。退出码不在输入面，不能单独判绿。
pub fn classify_gate(
    run: &RunVerdict,
    checks: &[GateCheckResult],
) -> (GateVerdict, Option<GateFailureCode>) {
    match run {
        RunVerdict::Halted { .. } | RunVerdict::Failed(crate::FailureCode::Timeout) => {
            (GateVerdict::NeedsHuman, Some(GateFailureCode::Timeout))
        }
        RunVerdict::Failed(crate::FailureCode::Infra) => {
            (GateVerdict::NeedsHuman, Some(GateFailureCode::Infra))
        }
        RunVerdict::Failed(crate::FailureCode::Bench) => {
            (GateVerdict::NeedsHuman, Some(GateFailureCode::Bench))
        }
        RunVerdict::Failed(crate::FailureCode::Subject) => {
            (GateVerdict::Fail, Some(GateFailureCode::Subject))
        }
        RunVerdict::InProgress => (GateVerdict::NeedsHuman, Some(GateFailureCode::Infra)),
        RunVerdict::Passed => classify_checks(checks),
    }
}

fn classify_checks(checks: &[GateCheckResult]) -> (GateVerdict, Option<GateFailureCode>) {
    if checks.is_empty() {
        return (GateVerdict::NeedsHuman, Some(GateFailureCode::Bench));
    }
    if checks
        .iter()
        .any(|c| c.required && matches!(c.status, GateCheckStatus::Fail))
    {
        return (GateVerdict::Fail, Some(GateFailureCode::Subject));
    }
    if let Some(skip) = checks
        .iter()
        .find(|c| c.required && c.status == GateCheckStatus::Skip)
    {
        let code = skip
            .skip_reason
            .map(SkipReason::failure_code)
            .unwrap_or(GateFailureCode::Bench);
        return (GateVerdict::NeedsHuman, Some(code));
    }
    (GateVerdict::Pass, None)
}

pub fn workspace_snapshot_digest(
    files: &[WorkspaceFile],
) -> std::result::Result<String, GateError> {
    let mut previous: Option<&str> = None;
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"af.gate.workspace_snapshot.v1\n");
    for file in files {
        validate_relative_path(&file.path)?;
        if previous.is_some_and(|old| old >= file.path.as_str()) {
            return Err(GateError::Invalid(
                "workspace_snapshot files 必须按相对路径严格排序且不得重复".into(),
            ));
        }
        require_digest(&file.sha256, "workspace_snapshot.files[].sha256")?;
        previous = Some(&file.path);
        push_length_prefixed(&mut canonical, file.path.as_bytes());
        push_length_prefixed(&mut canonical, file.length.to_string().as_bytes());
        push_length_prefixed(&mut canonical, file.sha256.as_bytes());
    }
    Ok(sha256_hex(&canonical))
}

fn validate_relative_path(path: &str) -> std::result::Result<(), GateError> {
    let candidate = Path::new(path);
    if path.is_empty()
        || candidate.is_absolute()
        || candidate.components().any(|part| {
            !matches!(part, Component::Normal(_) | Component::CurDir)
                || matches!(part, Component::CurDir)
        })
        || path.contains('\\')
    {
        return Err(GateError::Invalid(format!(
            "workspace_snapshot 路径不是规范相对路径: {path:?}"
        )));
    }
    Ok(())
}

fn push_length_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes.len().to_string().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(bytes);
    out.push(b'\n');
}

fn require_digest(value: &str, field: &str) -> std::result::Result<(), GateError> {
    if value.len() != SHA256_HEX_LEN || !is_lower_hex(value) {
        return Err(GateError::Invalid(format!(
            "{field} 必须是 64 位小写 sha256"
        )));
    }
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("门工件无效: {0}")]
    Invalid(String),
    #[error("CAS 内容冲突（禁覆盖）: {path}")]
    CasConflict { path: String },
    #[error("门工件 JSON: {0}")]
    Json(String),
    #[error("门工件 IO: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGateResult {
    pub content_sha256: String,
    pub path: PathBuf,
    pub persistent_bytes: u64,
}

/// 小型文件 CAS：地址 = 持久 JSON 字节 sha256；同字节重放幂等，异字节绝不覆盖。
#[derive(Debug, Clone)]
pub struct GateResultStore {
    root: PathBuf,
}

impl GateResultStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn persist(
        &self,
        result: &GateResultEnvelope,
    ) -> std::result::Result<StoredGateResult, GateError> {
        let bytes = result.persistent_bytes()?;
        let digest = sha256_hex(&bytes);
        let path = self.path_for(&digest)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::read(&path)? != bytes {
                    return Err(GateError::CasConflict {
                        path: path.display().to_string(),
                    });
                }
            }
            Err(err) => return Err(err.into()),
        }
        Ok(StoredGateResult {
            content_sha256: digest,
            persistent_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            path,
        })
    }

    pub fn load(&self, digest: &str) -> std::result::Result<GateResultEnvelope, GateError> {
        let path = self.path_for(digest)?;
        let bytes = fs::read(path)?;
        if sha256_hex(&bytes) != digest {
            return Err(GateError::CasConflict {
                path: self.path_for(digest)?.display().to_string(),
            });
        }
        let result: GateResultEnvelope =
            serde_json::from_slice(&bytes).map_err(|err| GateError::Json(err.to_string()))?;
        result.validate()?;
        Ok(result)
    }

    pub fn path_for(&self, digest: &str) -> std::result::Result<PathBuf, GateError> {
        require_digest(digest, "CAS digest")?;
        Ok(self
            .root
            .join("cas")
            .join("sha256")
            .join(&digest[..2])
            .join(format!("{digest}.json")))
    }
}

/// SKIP 计数投影，便于看板不必重算三项。
pub fn skip_counts(checks: &[GateCheckResult]) -> BTreeMap<&'static str, u64> {
    let accounting = SkipAccounting::of(checks);
    BTreeMap::from([
        (
            "environment_unavailable",
            accounting.environment_unavailable,
        ),
        ("explicitly_excluded", accounting.explicitly_excluded),
        ("not_applicable", accounting.not_applicable),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExecSignals, FailureCode, ManualClock, RunSpec};

    fn digest(ch: char) -> String {
        std::iter::repeat_n(ch, 64).collect()
    }

    fn check(id: &str, required: bool, status: GateCheckStatus) -> GateCheckResult {
        GateCheckResult {
            check_id: id.into(),
            required,
            status,
            skip_reason: (status == GateCheckStatus::Skip)
                .then_some(SkipReason::EnvironmentUnavailable),
            detail: String::new(),
        }
    }

    fn subject() -> GateSubject {
        GateSubject {
            kind: GateSubjectKind::GitCommit,
            repo: "example/repo".into(),
            version: "0123456789abcdef0123456789abcdef01234567".into(),
            files: Vec::new(),
        }
    }

    fn result(checks: Vec<GateCheckResult>, run: RunVerdict) -> GateResultEnvelope {
        let (verdict, failure_code) = classify_gate(&run, &checks);
        GateResultEnvelope::new(GateResult {
            producer_order_id: "wo-gate".into(),
            eval: GateEvalRef {
                ledger_id: "eval".into(),
                run_id: "run".into(),
            },
            suite: GateSuiteRef {
                lineage_id: "f1".into(),
                version: 1,
                snapshot_hash: digest('a'),
            },
            subject: subject(),
            recipe_digest: digest('b'),
            runner: GateRunnerRef {
                r#impl: "process/test".into(),
                digest: digest('c'),
                environment_digest: digest('d'),
            },
            inputs: Vec::new(),
            skip_accounting: SkipAccounting::of(&checks),
            checks,
            verdict,
            failure_code,
            evidence: Vec::new(),
            started_at_ms: 10,
            finished_at_ms: 20,
        })
    }

    #[test]
    fn suite_uses_catalog_freeze_fork_and_blind_barrier() {
        let clock = ManualClock::new(1);
        let (mut catalog, _) = Catalog::open_in_memory_with_clock("eval", clock.clone()).unwrap();
        catalog.set_current_recipe("secret", "DO-NOT-LEAK").unwrap();
        let definitions = vec![GateCheckDefinition {
            check_id: "fmt".into(),
            required: true,
            input: serde_json::json!({"command": "cargo fmt --check"}),
        }];
        let frozen = freeze_gate_suite(&mut catalog, "f1", &definitions).unwrap();
        let view = catalog
            .begin_run(&RunSpec {
                run_id: "run-1",
                lineage_id: "f1",
                version: 1,
                subject_id: "repo",
                subject_version: "0123456789abcdef0123456789abcdef01234567",
                input: &[],
                budget_ms: 100,
            })
            .unwrap();
        assert_eq!(gate_definitions(&view).unwrap(), definitions);
        assert_eq!(view.snapshot.hash, frozen.snapshot_hash.unwrap());
        assert!(view.current_recipe().is_err());
        clock.set(2);
        catalog.finish_run("run-1", ExecSignals::default()).unwrap();

        let mut changed = definitions.clone();
        changed[0].input = serde_json::json!({"command": "cargo fmt --all --check"});
        let v2 = fork_gate_suite(&mut catalog, "f1", 1, &changed).unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.parent.unwrap().version, 1);
        assert_eq!(
            catalog.benchmark("f1", 1).unwrap().cases[0].payload,
            serde_json::to_string(&definitions[0]).unwrap()
        );
    }

    #[test]
    fn tri_state_negative_matrix_never_washes_green() {
        let required_fail = vec![check("assert", true, GateCheckStatus::Fail)];
        assert_eq!(
            classify_gate(&RunVerdict::Passed, &required_fail),
            (GateVerdict::Fail, Some(GateFailureCode::Subject))
        );
        for (reason, code) in [
            (SkipReason::NotApplicable, GateFailureCode::Bench),
            (SkipReason::EnvironmentUnavailable, GateFailureCode::Infra),
            (SkipReason::ExplicitlyExcluded, GateFailureCode::Bench),
        ] {
            let mut skipped = check("required", true, GateCheckStatus::Skip);
            skipped.skip_reason = Some(reason);
            assert_eq!(
                classify_gate(&RunVerdict::Passed, &[skipped]),
                (GateVerdict::NeedsHuman, Some(code))
            );
        }
        for (run, code) in [
            (
                RunVerdict::Failed(FailureCode::Infra),
                GateFailureCode::Infra,
            ),
            (
                RunVerdict::Failed(FailureCode::Bench),
                GateFailureCode::Bench,
            ),
            (
                RunVerdict::Failed(FailureCode::Timeout),
                GateFailureCode::Timeout,
            ),
            (
                RunVerdict::Halted {
                    overlap: vec!["leak".into()],
                },
                GateFailureCode::Timeout,
            ),
        ] {
            assert_eq!(
                classify_gate(&run, &required_fail).0,
                GateVerdict::NeedsHuman
            );
            assert_eq!(classify_gate(&run, &required_fail).1, Some(code));
        }
        assert_eq!(
            classify_gate(&RunVerdict::Passed, &[]),
            (GateVerdict::NeedsHuman, Some(GateFailureCode::Bench))
        );
        assert!(result(required_fail, RunVerdict::Passed).validate().is_ok());
    }

    #[test]
    fn optional_skip_is_accounted_but_does_not_block_required_pass() {
        let checks = vec![
            check("required", true, GateCheckStatus::Pass),
            check("optional", false, GateCheckStatus::Skip),
        ];
        let artifact = result(checks, RunVerdict::Passed);
        assert_eq!(artifact.payload.verdict, GateVerdict::Pass);
        assert_eq!(artifact.payload.skip_accounting.environment_unavailable, 1);
        artifact.validate().unwrap();
    }

    #[test]
    fn subject_validation_rejects_short_sha_fake_and_unsorted_workspace() {
        let mut short = result(
            vec![check("c", true, GateCheckStatus::Pass)],
            RunVerdict::Passed,
        );
        short.payload.subject.version = "abc123".into();
        assert!(short.validate().is_err());

        let files = vec![
            WorkspaceFile {
                path: "z.rs".into(),
                length: 1,
                sha256: digest('a'),
            },
            WorkspaceFile {
                path: "a.rs".into(),
                length: 1,
                sha256: digest('b'),
            },
        ];
        let mut workspace = result(
            vec![check("c", true, GateCheckStatus::Pass)],
            RunVerdict::Passed,
        );
        workspace.payload.subject.kind = GateSubjectKind::WorkspaceSnapshot;
        workspace.payload.subject.files = files;
        workspace.payload.subject.version = digest('0');
        assert!(workspace.validate().is_err());
    }

    #[test]
    fn workspace_digest_is_sorted_relative_path_length_and_digest() {
        let files = vec![
            WorkspaceFile {
                path: "a.rs".into(),
                length: 3,
                sha256: digest('a'),
            },
            WorkspaceFile {
                path: "dir/b.rs".into(),
                length: 5,
                sha256: digest('b'),
            },
        ];
        let version = workspace_snapshot_digest(&files).unwrap();
        let mut workspace = result(
            vec![check("c", true, GateCheckStatus::Pass)],
            RunVerdict::Passed,
        );
        workspace.payload.subject.kind = GateSubjectKind::WorkspaceSnapshot;
        workspace.payload.subject.version = version;
        workspace.payload.subject.files = files;
        workspace.validate().unwrap();
    }

    #[test]
    fn cas_persists_exact_bytes_and_refuses_overwrite() {
        let root =
            std::env::temp_dir().join(format!("af-gate-cas-{}-{}", std::process::id(), line!()));
        let _ = fs::remove_dir_all(&root);
        let store = GateResultStore::new(&root);
        let artifact = result(
            vec![check("c", true, GateCheckStatus::Fail)],
            RunVerdict::Passed,
        );
        let bytes = artifact.persistent_bytes().unwrap();
        let stored = store.persist(&artifact).unwrap();
        assert_eq!(stored.content_sha256, sha256_hex(&bytes));
        assert_eq!(fs::read(&stored.path).unwrap(), bytes);
        assert_eq!(store.persist(&artifact).unwrap(), stored);
        assert_eq!(store.load(&stored.content_sha256).unwrap(), artifact);

        fs::write(&stored.path, b"tampered").unwrap();
        let before = fs::read(&stored.path).unwrap();
        assert!(store.persist(&artifact).is_err());
        assert_eq!(
            fs::read(&stored.path).unwrap(),
            before,
            "负例不得覆盖既有 CAS 字节"
        );
        assert!(store.load(&stored.content_sha256).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
