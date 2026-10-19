//! 库入口：装载管线 + 错误分类学 + 类型化结构。

pub mod error;
pub mod loader;
pub mod model;

pub use error::LoadError;
pub use loader::{load_str, load_value, LoadedBundle, SUPPORTED_SCHEMA_VERSION};
pub use model::{
    AndonRule, ArtifactField, ArtifactSchema, ComboParams, CountParams, CmpOp, DwellParams,
    EscalateAction, EscalateStep, EscalationParams, EvalFunction, FieldType, FingerprintParams,
    GraphTemplateRef, Line, RatioParams, Rule, RuleKind, RuleParams, Severity, ThresholdParams,
    TimeWindowParams, Tool, ToolDriver,
};
