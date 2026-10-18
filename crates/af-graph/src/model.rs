//! 图模板数据结构（serde 定义）。节点四型开放为 String 以便将来注册扩展；
//! 人工/循环/map 是节点上的一等构造字段，与 kind 正交。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 节点 kind 的首发四型常量。kind 本身是开放 String：校验器不拒绝未知 kind，
/// 将来注册扩展类型（如 "rerank"）不改 schema。
pub mod kind {
    pub const TASK: &str = "task";
    pub const GATE: &str = "gate";
    pub const CHECK: &str = "check";
    pub const TRANSFORM: &str = "transform";

    /// 首发四型（仅用于诊断展示，不做白名单校验）。
    pub const BUILTIN: [&str; 4] = [TASK, GATE, CHECK, TRANSFORM];
}

/// 一条产线的图模板：静态拓扑定义，住在产线定义仓库，版本化管理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTemplate {
    /// 图版本：一等键（修正案 7）。写入每个工件元数据 / span / 发布投影头。
    pub graph_version: String,
    pub name: String,
    /// 入口节点 id。
    pub entry: String,
    pub nodes: Vec<Node>,
    /// 边 = 工件引用：生产者节点的产出工件 → 消费者节点的消费工件。
    pub edges: Vec<Edge>,
    /// 可复用子图（循环体 / map 体）。键为子图名。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub subgraphs: BTreeMap<String, SubGraph>,
}

/// 图节点。kind 开放为 String；`human` / `loop_spec` / `map_spec` 是一等构造，
/// 与 kind 正交（任何 kind 的节点都可携带）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    /// 节点类型，首发四型 task/gate/check/transform，开放 String 以便注册扩展。
    pub kind: String,
    /// 一等人工节点标记：挂上即该节点由人工工位执行（Gate+Transform 双语义）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human: Option<HumanSpec>,
    /// 受控循环：循环体=独立子图 + 携带状态有界 + 必须声明安全点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_spec: Option<ControlledLoop>,
    /// map 原语：遍历工件集合，运行时定基数 fan-out（对标 Airflow Dynamic Task
    /// Mapping）；子图实例化要求子图已版本化（已审定则实例化不算改拓扑）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_spec: Option<MapSpec>,
}

/// 人工工位规格：分派路由键 + 资质角色 + 可选会审 quorum（k-of-n）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanSpec {
    /// 人工工位（人工任务账本分派路由键）。
    pub station: String,
    /// 资质 / 角色（SOP 资质门）。
    pub role: String,
    /// 会审人数下限（k-of-n），缺省单审。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quorum: Option<u32>,
}

/// 受控循环声明。设计纪律：循环体的环是这个构造自己的事——静态 DAG 检查对
/// 循环体子图豁免（否则合法循环不存在），但必须用安全点 + 有界来"受控"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlledLoop {
    /// 循环体：独立子图名（必须已声明）。
    pub body: String,
    /// 迭代上限（>0）。防活锁：人机往返循环预算落在图定义里。
    pub max_iterations: u32,
    /// 跨迭代携带的状态，必须有界（防事件历史无限膨胀触发分片）。
    pub carried_state: CarriedState,
    /// 安全点列表：每次迭代必须经过的检查位置。缺安全点 = 拒绝编译。
    pub safety_points: Vec<SafetyPoint>,
}

/// 跨迭代携带状态。max_items / max_bytes 至少声明一个，否则视为无界。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarriedState {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

impl CarriedState {
    /// 是否有界：至少声明一个上界。
    pub fn is_bounded(&self) -> bool {
        self.max_items.is_some() || self.max_bytes.is_some()
    }
}

/// 安全点：循环体内一个必须存在的检查位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyPoint {
    /// 循环体子图内的节点 id。
    pub node: String,
    /// 安全点类型：check 节点 / gate 节点 / 人工节点。
    pub kind: SafetyPointKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyPointKind {
    Check,
    Gate,
    Human,
}

/// map 原语声明：遍历一个工件集合，运行时确定基数，展开 N 个子图实例。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapSpec {
    /// 每个元素跑一遍的子图（必须已版本化：graph_version 有值）。
    pub body: String,
    /// 被遍历的工件集合引用（形如 "节点.工件名"）。
    pub over: String,
    /// 子图实例并发上限（durable 引擎 child 批次的并发控制）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fanout: Option<u32>,
}

/// 边：工件引用。`from.output → to.input`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// 生产者节点 id。
    pub from: String,
    /// 产出工件名。
    pub output: String,
    /// 消费者节点 id。
    pub to: String,
    /// 消费工件名。
    pub input: String,
}

impl std::fmt::Display for Edge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{} -> {}.{}", self.from, self.output, self.to, self.input)
    }
}

/// 可复用子图：独立的节点/边命名空间，自带入口。
/// map 体必须已版本化（校验规则⑤）；循环体的环被循环构造接管（DAG 检查豁免）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGraph {
    /// 子图自身的版本。None = 未审定草稿——map 引用它会被拒绝。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_version: Option<String>,
    pub entry: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
