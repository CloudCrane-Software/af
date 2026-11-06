//! router_liveness —— 收件箱活性安灯的 af 产品链（Rust 面）读数腿：把
//! PoC-D event-router `_trigger_liveness_andon()` 的判定语义**逐字**搬进
//! af-order，治风险#7『新链建成没事件也要报活』（事件流静默 268h 前科）。
//! 对齐正本：外层工作区 `tools/event-router.py`；本仓版本控制镜像 =
//! `poc/poc-super-agent/tests/event-router-poCD.diff`（本机核对的取件面）。
//!
//! # 判据（与 PoC-D 侧逐字对齐——两腿对同一注册表/收件箱必须同读数）
//!
//! - 阈值来自注册表 `liveness.inbox_silent_after_min`，**显式 None 判**
//!   （对齐 PoC-D `if thr_min is None: thr_min = 240`）：段缺席/字段缺席/
//!   显式 null ⇒ 回落 [`DEFAULT_SILENT_AFTER_MIN`]（240）；**0 是合法阈值**
//!   （立即安灯），落 `Some(0)` 分支——没有 falsy 陷阱可踩。
//! - 静默时长 = `now - max(收件箱最新投递 mtime, 本腿首跑锚点)`；
//!   `silent > threshold*60` **严格大于**才 amber（恰好压线 = green）。
//! - 收件箱扫描 = `inbox/*/*.json` 的 glob 语义镜像：两级名字都以 `.` 起
//!   头的跳过（glob 不匹配点开头）；单文件 mtime 读失败跳过；收件箱目录
//!   缺席/读失败按空收件箱（glob 静默空，不炸读数腿）。
//! - 注册表 schema 必须是 `af.trigger.registry.v1`：PoC-D 侧注册表读不到/
//!   schema 不识别时消费腿早退、**安灯整轮不评估**，本腿同口径 fail-closed
//!   返回 [`LivenessError::Registry`]，不产读数（不假绿）。
//!
//! # 起算锚点（Rust 腿无进程态 ⇒ 可持久化锚点文件）
//!
//! PoC-D 的 `started_at` 是进程内状态（首跑写下、复用不刷新）；本腿无常驻
//! 进程，锚点落 `<triggers-home>/liveness-anchor.json`（与 consumed.jsonl
//! 同目录）：**首跑创建（锚点=当次 now）、后续跑复用**，永不刷新——静默
//! 时长单调累积，『收件箱自建成就没来过事件也算静默，但从建成起算，
//! 不冤枉刚部署』。锚点文件损坏（非 JSON / 缺字段 / 负值）= fail-closed
//! [`LivenessError::Anchor`]：**静默重建锚点会把静默时钟清零**，重造本安灯
//! 要治的 268h 盲窗；与 [`crate::router::load_consumed`] 坏行 fail-closed
//! 同纪律（游标/锚点都是真源，真源损坏不容半行糊弄）。处置 = 人工核查后
//! 显式删锚点（知情重置，重置即重新起算）。
//!
//! # 只读纪律（安灯是观测，不是门）
//!
//! 本模块除锚点文件外**零写面**：不写 consumed.jsonl、不写账本、不派单、
//! 不改任何业务状态；amber 是读数不是失败（不引入静默即失败）。快照落盘
//! 由调用方指定路径（[`write_snapshot`]：tmp + rename 原子换，PoC-D
//! `os.replace` 同款），字段名/语义/插入序与 PoC-D
//! `状态观测/trigger-inbox-liveness.json` 逐字对齐（10 字段；`tool` 取
//! [`SNAPSHOT_TOOL`] = "af-order/router-liveness" 区分写腿）。
//!
//! # 取舍登记（对齐基准面外显）
//!
//! - [`crate::router::TriggerRegistry`] 仍**只解 sources 段**（消费腿宽松
//!   解析契约零触碰，router.rs 本批零改动）；本模块用独立最小视图
//!   [`LivenessRegistryView`] 只解 liveness 段——任务书选项②，不碰既有
//!   struct 的 serde 行为。
//! - 注册表 `inbox_silent_after_min` 非整数（浮点/字符串/布尔）⇒ 本腿
//!   [`LivenessError::Registry`] fail-closed；PoC-D 侧 `float()` 会把数值
//!   字符串/布尔强转出读数（Python 松面）。两腿对**整数阈值**（正本形态）
//!   逐字一致；非整数属坏形，拒比猜安全——如实登记为口径差。
//! - 锚点 per-leg：Rust 腿锚点独立于 PoC-D 腿进程 `started_at`，两腿各自
//!   从各自首跑起算（同款语义，非共享锚）。

use std::path::{Path, PathBuf};

use chrono::TimeZone as _;
use serde::{Deserialize, Serialize};

/// 注册表运行位文件名（PoC-D `TRIGGER_REGISTRY_F` 同位：<home>/registry.json）。
pub const REGISTRY_FILE: &str = "registry.json";
/// 收件箱目录名（PoC-D `TRIGGER_INBOX_D` 同位：<home>/inbox）。
pub const INBOX_DIR: &str = "inbox";
/// 起算锚点文件名（与 consumed.jsonl 同目录；本腿唯一写面）。
pub const ANCHOR_FILE: &str = "liveness-anchor.json";
/// 阈值缺省回落（PoC-D `if thr_min is None: thr_min = 240` 同值）。
pub const DEFAULT_SILENT_AFTER_MIN: i64 = 240;
/// 注册表 schema 契约名（PoC-D `_trigger_registry` 同判据）。
pub const REGISTRY_SCHEMA: &str = "af.trigger.registry.v1";
/// 快照 `tool` 字段值：区分 PoC-D 腿（"event-router/trigger-leg"）。
pub const SNAPSHOT_TOOL: &str = "af-order/router-liveness";
/// amber 读数安灯码（PoC-D 同码）。
pub const ALARM_CODE_SILENT: &str = "trigger-inbox-silent";
/// green 读数安灯码（PoC-D 同码）。
pub const ALARM_CODE_LIVE: &str = "trigger-inbox-live";

/// 只解 liveness 段的注册表最小视图（任务书选项②：独立视图类型，不碰
/// [`crate::router::TriggerRegistry`] 的宽松解析契约；未知段/未知字段一律
/// 忽略——数据文件读侧）。schema 缺席视同不符（fail-closed，见
/// [`evaluate_liveness`]）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LivenessRegistryView {
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub liveness: Option<LivenessSection>,
}

/// 注册表 `liveness` 段（正本还含 `$comment` 等注释字段，宽松忽略）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LivenessSection {
    /// 收件箱静默安灯阈值（分钟）。`None` = 段内缺席/显式 null ⇒ 回落 240；
    /// `Some(0)` = 立即安灯的合法阈值。
    #[serde(default)]
    pub inbox_silent_after_min: Option<i64>,
}

/// 从注册表文件文本解 liveness 视图（宽松：未知字段忽略；schema/liveness
/// 段形状坏 = fail-closed）。
pub fn parse_liveness_view(text: &str) -> Result<LivenessRegistryView, LivenessError> {
    serde_json::from_str(text).map_err(|err| {
        LivenessError::Registry(format!("注册表不是合法 af.trigger.registry 视图: {err}"))
    })
}

/// 读注册表运行位并解 liveness 视图。
pub fn load_liveness_view(path: &Path) -> Result<LivenessRegistryView, LivenessError> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        LivenessError::Registry(format!("注册表读取失败 {}: {err}", path.display()))
    })?;
    parse_liveness_view(&text)
}

/// 阈值解析：显式 None 判（对齐 PoC-D `if thr_min is None: 240`）。
/// `Some(0)` 原样保留——0 是合法阈值（立即安灯），不是缺省。
pub fn resolve_threshold_min(view: &LivenessRegistryView) -> i64 {
    match view
        .liveness
        .as_ref()
        .and_then(|section| section.inbox_silent_after_min)
    {
        Some(minutes) => minutes,
        // 显式 None 判：段缺席/字段缺席/显式 null 才回落；0 走 Some 分支。
        None => DEFAULT_SILENT_AFTER_MIN,
    }
}

/// 安灯读数状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivenessStatus {
    Green,
    Amber,
}

impl LivenessStatus {
    /// 快照 `status` 字段值（PoC-D 同词）。
    pub fn as_str(self) -> &'static str {
        match self {
            LivenessStatus::Green => "green",
            LivenessStatus::Amber => "amber",
        }
    }
}

/// 起算锚点来源：`max(最新投递 mtime, 首跑锚点)` 哪边胜出（平局记
/// `newest_delivery`，对齐 Python `max(newest, started_at)` 首参胜平局）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSource {
    /// 收件箱最新投递 mtime 起算（有事件且比首跑新）。
    NewestDelivery,
    /// 本腿首跑锚点起算（收件箱空，或所有投递都比首跑旧）。
    FirstRunAnchor,
}

impl AnchorSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AnchorSource::NewestDelivery => "newest_delivery",
            AnchorSource::FirstRunAnchor => "first_run_anchor",
        }
    }
}

/// 一次活性安灯读数（观测面：判定输出；amber 是读数不是错误）。
#[derive(Debug, Clone, PartialEq)]
pub struct LivenessReading {
    /// green/amber。
    pub status: LivenessStatus,
    /// 静默秒数 = now − since（浮点，同 PoC-D `silent_s`）。
    pub silent_s: f64,
    /// 阈值分钟（显式 None 判解析后的值；0 合法）。
    pub threshold_min: i64,
    /// 起算时刻（epoch 秒）= max(最新投递 mtime, 首跑锚点)。
    pub since_epoch_s: f64,
    /// 起算来源（newest_delivery / first_run_anchor）。
    pub anchor_source: AnchorSource,
    /// 收件箱目录路径。
    pub inbox_dir: PathBuf,
    /// 读数时刻（epoch 秒）。
    pub now_epoch_s: f64,
    /// 读数时刻本地时区 `%Y-%m-%d %H:%M:%S`（PoC-D `time.strftime` 同格式）。
    pub ts_local: String,
    /// 本轮是否首跑新建了锚点（Rust 腿观测字段；不进快照）。
    pub anchor_created: bool,
}

impl LivenessReading {
    /// 安灯码（PoC-D 同码：amber=trigger-inbox-silent / green=trigger-inbox-live）。
    pub fn alarm_code(&self) -> &'static str {
        match self.status {
            LivenessStatus::Amber => ALARM_CODE_SILENT,
            LivenessStatus::Green => ALARM_CODE_LIVE,
        }
    }

    /// 摘要消息（PoC-D 模板逐字：amber 小时一位小数；green 分钟零位小数）。
    pub fn message(&self) -> String {
        match self.status {
            LivenessStatus::Amber => format!(
                "Trigger 收件箱静默 {:.1}h（>阈 {}min）——Fast Path 断供：无事件可路由；\
                 恢复=事件源重投/检查 webhook 出口",
                self.silent_s / 3600.0,
                self.threshold_min
            ),
            LivenessStatus::Green => format!(
                "收件箱活性正常（最新投递 {:.0}min 前，阈 {}min）",
                self.silent_s / 60.0,
                self.threshold_min
            ),
        }
    }

    /// 快照 JSON（PoC-D `状态观测/trigger-inbox-liveness.json` 同款：10 字段、
    /// 同插入序、indent=1、非 ASCII 不转义；数值字段 = Python `int()` 朝零
    /// 截断）。`tool` 取 [`SNAPSHOT_TOOL`] 区分写腿。
    pub fn snapshot_json(&self) -> String {
        // 字符串转义交给 serde_json（与 json.dump ensure_ascii=False 同口径：
        // 只转义控制字符/引号/反斜杠，非 ASCII 原样）。
        let esc = |text: &str| serde_json::to_string(text).expect("字符串序列化不失败");
        let fields: [(String, String); 10] = [
            ("ts".into(), esc(&self.ts_local)),
            ("ts_epoch".into(), py_int(self.now_epoch_s).to_string()),
            ("tool".into(), esc(SNAPSHOT_TOOL)),
            ("status".into(), esc(self.status.as_str())),
            ("message".into(), esc(&self.message())),
            ("alarm_code".into(), esc(self.alarm_code())),
            (
                "newest_delivery_age_s".into(),
                py_int(self.silent_s).to_string(),
            ),
            ("threshold_min".into(), self.threshold_min.to_string()),
            (
                "inbox_dir".into(),
                esc(&self.inbox_dir.display().to_string()),
            ),
            ("since_epoch".into(), py_int(self.since_epoch_s).to_string()),
        ];
        let mut out = String::from("{\n");
        for (index, (key, value)) in fields.iter().enumerate() {
            out.push_str(&format!(" \"{key}\": {value}"));
            if index + 1 < fields.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push('}');
        out
    }
}

/// Python `int()` 朝零截断（-0.5 → 0，不是 -0；与快照数值字段口径对齐）。
fn py_int(value: f64) -> i64 {
    let truncated = value.trunc();
    if truncated == 0.0 {
        0
    } else {
        truncated as i64
    }
}

/// 活性安灯读数腿失败（类型化，fail-closed：读数面真源损坏不产读数不假绿）。
#[derive(Debug, thiserror::Error)]
pub enum LivenessError {
    /// 注册表面失败（读不到 / 非 JSON / schema 不识别 / 阈值非整数坏形）。
    #[error("活性安灯注册表面失败: {0}")]
    Registry(String),
    /// 起算锚点失败（损坏/不可写；fail-closed 拒静默重建——静默时钟不清零）。
    #[error("活性安灯起算锚点失败: {0}")]
    Anchor(String),
    /// 快照落盘失败（tmp+rename 原子换）。
    #[error("活性安灯快照落盘失败: {0}")]
    Write(String),
}

/// 锚点文件线格式（`<triggers-home>/liveness-anchor.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AnchorFile {
    first_run_epoch_s: f64,
}

/// 读锚点：文件缺席 = 首跑，写锚点=now 并返回 (now, true)；在位 = 复用
/// （不刷新）。损坏（非 JSON/缺字段/负值）= fail-closed [`LivenessError::Anchor`]。
fn load_or_create_anchor(
    triggers_home: &Path,
    now_epoch_s: f64,
) -> Result<(f64, bool), LivenessError> {
    let path = triggers_home.join(ANCHOR_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let anchor: AnchorFile = serde_json::from_str(&text).map_err(|err| {
                LivenessError::Anchor(format!(
                    "锚点 {} 损坏（fail-closed 拒静默重建，静默时钟不清零；\
                     处置=人工核查后显式删锚点）: {err}",
                    path.display()
                ))
            })?;
            if !(anchor.first_run_epoch_s.is_finite() && anchor.first_run_epoch_s >= 0.0) {
                return Err(LivenessError::Anchor(format!(
                    "锚点 {} 的 first_run_epoch_s 非法（须为非负有限数）: {}",
                    path.display(),
                    anchor.first_run_epoch_s
                )));
            }
            // 后续跑复用锚点（不刷新起算时刻）。
            Ok((anchor.first_run_epoch_s, false))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            // 首跑：创建锚点（本模块唯一写面，tmp+rename 原子落）；创建失败
            // 归锚点面（无锚点不起算，不产读数）。
            write_json_atomic(
                &path,
                &AnchorFile {
                    first_run_epoch_s: now_epoch_s,
                },
            )
            .map_err(|err| match err {
                LivenessError::Write(detail) => {
                    LivenessError::Anchor(format!("首跑锚点创建失败: {detail}"))
                }
                other => other,
            })?;
            Ok((now_epoch_s, true))
        }
        Err(err) => Err(LivenessError::Anchor(format!(
            "锚点 {} 读取失败 {}: {err}",
            path.display(),
            err.kind()
        ))),
    }
}

/// tmp + rename 原子写（PoC-D `os.replace` 同款；rename(2) 在同目录内原子）。
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), LivenessError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|err| LivenessError::Write(format!("创建 {}: {err}", parent.display())))?;
    let text = serde_json::to_string(value)
        .map_err(|err| LivenessError::Write(format!("锚点序列化失败: {err}")))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)
        .map_err(|err| LivenessError::Write(format!("写 {} 失败: {err}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|err| {
        LivenessError::Write(format!(
            "原子换 {} ← {} 失败: {err}",
            path.display(),
            tmp.display()
        ))
    })?;
    Ok(())
}

/// 扫收件箱最新投递 mtime（glob `inbox/*/*.json` 语义镜像：两级点开头名
/// 跳过；mtime 读失败跳过；目录缺席/读失败按空 ⇒ 0.0）。
fn scan_newest_delivery_mtime(inbox_dir: &Path) -> f64 {
    let mut newest = 0.0f64;
    let Ok(sources) = std::fs::read_dir(inbox_dir) else {
        return newest; // glob 静默空：目录缺席/不可读 = 无投递
    };
    for source in sources.flatten() {
        let source_name = source.file_name();
        let Some(source_name) = source_name.to_str() else {
            continue;
        };
        if source_name.starts_with('.') {
            continue; // glob `*` 不匹配点开头（Python glob 语义）
        }
        let Ok(deliveries) = std::fs::read_dir(source.path()) else {
            continue;
        };
        for delivery in deliveries.flatten() {
            let file_name = delivery.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if name.starts_with('.') || !name.ends_with(".json") {
                continue;
            }
            if let Ok(meta) = delivery.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if let Ok(secs) = mtime.duration_since(std::time::UNIX_EPOCH) {
                        newest = newest.max(secs.as_secs_f64());
                    }
                }
            }
        }
    }
    newest
}

/// 读数时刻本地时区格式化（PoC-D `time.strftime("%Y-%m-%d %H:%M:%S")` 同款，
/// 本地时区、秒精度；失败回落 epoch 串——读数腿不因排版炸）。
fn format_ts_local(now_epoch_s: f64) -> String {
    let secs = now_epoch_s.trunc();
    let nanos = ((now_epoch_s - secs) * 1e9).clamp(0.0, 999_999_999.0) as u32;
    match chrono::Local.timestamp_opt(secs as i64, nanos).single() {
        Some(moment) => moment.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("@{secs:.0}"),
    }
}

/// 评估收件箱活性安灯（只读判定 + 首跑锚点这一处写）：
/// 注册表（schema 门 + 阈值）→ 锚点（首跑创建/复用）→ 收件箱最新投递
/// mtime → `since = max(newest, anchor)`、`silent = now − since`、
/// `silent > threshold*60` 严格大于即 amber。amber 是读数不是错误。
///
/// `now_epoch_s` 由调用方给（对齐 PoC-D `time.time()`；显式参数供测试/复现）。
pub fn evaluate_liveness(
    triggers_home: &Path,
    now_epoch_s: f64,
) -> Result<LivenessReading, LivenessError> {
    // 注册表面：读不到/坏形/schema 不识别 = 不产读数（PoC-D 安灯随消费腿
    // 早退同口径，fail-closed 不假绿）。
    let view = load_liveness_view(&triggers_home.join(REGISTRY_FILE))?;
    if view.schema.as_deref() != Some(REGISTRY_SCHEMA) {
        return Err(LivenessError::Registry(format!(
            "注册表 schema 不识别: {:?}（期望 {REGISTRY_SCHEMA}，安灯不评估）",
            view.schema
        )));
    }
    let threshold_min = resolve_threshold_min(&view);

    // 起算锚点：首跑创建、后续复用（唯一写面）。
    let (anchor, anchor_created) = load_or_create_anchor(triggers_home, now_epoch_s)?;

    // since = max(最新投递 mtime, 首跑锚点)；平局记 newest_delivery
    // （Python max 首参胜平局同语义）。
    let inbox_dir = triggers_home.join(INBOX_DIR);
    let newest = scan_newest_delivery_mtime(&inbox_dir);
    let anchor_source = if newest >= anchor {
        AnchorSource::NewestDelivery
    } else {
        AnchorSource::FirstRunAnchor
    };
    let since = newest.max(anchor);
    let silent_s = now_epoch_s - since;
    let status = if silent_s > threshold_min as f64 * 60.0 {
        LivenessStatus::Amber
    } else {
        LivenessStatus::Green
    };
    Ok(LivenessReading {
        status,
        silent_s,
        threshold_min,
        since_epoch_s: since,
        anchor_source,
        inbox_dir,
        now_epoch_s,
        ts_local: format_ts_local(now_epoch_s),
        anchor_created,
    })
}

/// 快照落盘（调用方指定路径；tmp+rename 原子换，PoC-D 同款；父目录自动建）。
pub fn write_snapshot(reading: &LivenessReading, path: &Path) -> Result<(), LivenessError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|err| LivenessError::Write(format!("创建 {}: {err}", parent.display())))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, reading.snapshot_json())
        .map_err(|err| LivenessError::Write(format!("写 {} 失败: {err}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|err| {
        LivenessError::Write(format!(
            "原子换 {} ← {} 失败: {err}",
            path.display(),
            tmp.display()
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_from(text: &str) -> LivenessRegistryView {
        parse_liveness_view(text).expect("合法注册表视图")
    }

    /// 阈值显式 None 判：段缺席/字段缺席/显式 null ⇒ 240；Some(0) ⇒ 0。
    #[test]
    fn threshold_explicit_none_check() {
        let absent_section = view_from(r#"{"schema":"af.trigger.registry.v1","sources":[]}"#);
        assert_eq!(resolve_threshold_min(&absent_section), 240);
        let null_section =
            view_from(r#"{"schema":"af.trigger.registry.v1","liveness":null,"sources":[]}"#);
        assert_eq!(resolve_threshold_min(&null_section), 240);
        let empty_section = view_from(r#"{"schema":"af.trigger.registry.v1","liveness":{}}"#);
        assert_eq!(resolve_threshold_min(&empty_section), 240);
        let null_field = view_from(
            r#"{"schema":"af.trigger.registry.v1","liveness":{"inbox_silent_after_min":null}}"#,
        );
        assert_eq!(resolve_threshold_min(&null_field), 240);
        // 0 是合法阈值（立即安灯），不是缺省。
        let zero = view_from(
            r#"{"schema":"af.trigger.registry.v1","liveness":{"inbox_silent_after_min":0}}"#,
        );
        assert_eq!(resolve_threshold_min(&zero), 0);
        let configured = view_from(
            r#"{"schema":"af.trigger.registry.v1","liveness":{"inbox_silent_after_min":30,
                "$comment":"注释字段宽松忽略"}}"#,
        );
        assert_eq!(resolve_threshold_min(&configured), 30);
    }

    /// 非整数阈值（浮点/字符串/布尔）= 坏形 fail-closed（口径差如实登记：
    /// PoC-D float() 会强转，本腿拒猜）。
    #[test]
    fn threshold_non_integer_shape_rejected() {
        for bad in [
            r#"{"liveness":{"inbox_silent_after_min":240.5}}"#,
            r#"{"liveness":{"inbox_silent_after_min":"240"}}"#,
            r#"{"liveness":{"inbox_silent_after_min":true}}"#,
        ] {
            assert!(parse_liveness_view(bad).is_err(), "应拒坏形: {bad}");
        }
    }

    /// 快照 10 字段同序同义 + Python int() 朝零截断。
    #[test]
    fn snapshot_shape_matches_pocd_fields() {
        let reading = LivenessReading {
            status: LivenessStatus::Amber,
            silent_s: 1079.9,
            threshold_min: 240,
            since_epoch_s: 1_700_000_000.25,
            anchor_source: AnchorSource::FirstRunAnchor,
            inbox_dir: PathBuf::from("/tmp/triggers/inbox"),
            now_epoch_s: 1_700_001_080.15,
            ts_local: "2026-11-04 12:00:00".into(),
            anchor_created: false,
        };
        let text = reading.snapshot_json();
        let value: serde_json::Value = serde_json::from_str(&text).expect("合法 JSON");
        let keys: Vec<&str> = value
            .as_object()
            .expect("对象")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "ts",
                "ts_epoch",
                "tool",
                "status",
                "message",
                "alarm_code",
                "newest_delivery_age_s",
                "threshold_min",
                "inbox_dir",
                "since_epoch",
            ],
            "字段名与插入序 = PoC-D 快照正本"
        );
        assert_eq!(value["tool"], SNAPSHOT_TOOL);
        assert_eq!(value["status"], "amber");
        assert_eq!(value["alarm_code"], "trigger-inbox-silent");
        // int() 朝零截断：1079.9 → 1079；1_700_001_080.15 → ...080；.25 → ...000。
        assert_eq!(value["newest_delivery_age_s"], 1079);
        assert_eq!(value["ts_epoch"], 1_700_001_080);
        assert_eq!(value["since_epoch"], 1_700_000_000);
        // 消息模板逐字（amber 小时一位小数）。
        assert_eq!(
            value["message"].as_str().expect("str"),
            "Trigger 收件箱静默 0.3h（>阈 240min）——Fast Path 断供：无事件可路由；\
             恢复=事件源重投/检查 webhook 出口"
        );
        // 负静默（时钟偏移面）截断朝零：-0.5 → 0（Python int() 同口径）。
        assert_eq!(py_int(-0.5), 0);
        assert_eq!(py_int(0.5), 0);
        assert_eq!(py_int(-1.5), -1);
    }

    /// green 消息模板逐字（分钟零位小数）。
    #[test]
    fn green_message_template() {
        let reading = LivenessReading {
            status: LivenessStatus::Green,
            silent_s: 120.0,
            threshold_min: 240,
            since_epoch_s: 1_700_000_000.0,
            anchor_source: AnchorSource::NewestDelivery,
            inbox_dir: PathBuf::from("/tmp/triggers/inbox"),
            now_epoch_s: 1_700_000_120.0,
            ts_local: "2026-11-04 12:02:00".into(),
            anchor_created: false,
        };
        assert_eq!(
            reading.message(),
            "收件箱活性正常（最新投递 2min 前，阈 240min）"
        );
        assert_eq!(reading.alarm_code(), "trigger-inbox-live");
    }

    /// 本地时区 ts 格式（秒精度；与 time.strftime 同格式串）。
    #[test]
    fn ts_local_format() {
        let ts = format_ts_local(1_700_000_000.0);
        assert_eq!(ts.len(), 19, "yyyy-MM-dd HH:mm:ss：{ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], " ");
        assert_eq!(&ts[13..14], ":");
    }
}
