//! DSPy 程序 IR v1 的领域类型。
//!
//! 字段名、输入/输出归属和类型都在这里。DSPy JSON 存档的 signature.fields
//! 只有 `prefix` 与 `description`，名字由本结构按位置补回。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::{ErrorCode, IrError};

/// 签名上的一个输入或输出字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// 字段名。空字符串在校验时是 DI1001，不静默丢掉。
    pub name: String,
    /// 提示词前缀。对应 DSPy `json_schema_extra["prefix"]`。
    pub prefix: String,
    /// 字段说明。对应存档里的 `description`（DSPy 内存键是 `desc`）。
    pub description: String,
    /// 类型名，例如 `str`、`bool`、`list[str]`。存档不带这个键，真相在 IR。
    pub type_name: String,
}

/// 一张签名：指令加上输入字段表、输出字段表。
///
/// DSPy `Signature.fields` 的顺序是先全部输入、再全部输出。和存档互转时按这个顺序对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub name: String,
    pub instructions: String,
    pub inputs: Vec<Field>,
    pub outputs: Vec<Field>,
}

impl Signature {
    /// 存档对齐用的字段顺序：输入在前，输出在后。
    pub fn ordered_fields(&self) -> impl Iterator<Item = &Field> {
        self.inputs.iter().chain(self.outputs.iter())
    }
}

/// 一条 few-shot demo。`values` 的键必须是所属签名的字段名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Demo {
    pub id: String,
    /// 指向 [`Signature::name`]。
    pub signature: String,
    pub values: BTreeMap<String, serde_json::Value>,
}

/// 挂在 predictor 上的调用配置。
///
/// DSPy 的 `temperature` 是小数。这里用毫单位整数：`0.7` 写成 `700`。
/// 三个键都是 `Option`：缺键和 `null` 都是未设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictorConfig {
    pub temperature_millis: Option<i64>,
    pub rollout_id: Option<i64>,
    /// 采样条数。`null` 表示未设置；有值时必须 ≥ 1。
    pub n: Option<u32>,
}

/// predictor 绑定的语言模型。只保留 DSPy `BaseLM.dump_state` 里稳定的四个键。
///
/// 存档里的 `_dspy_lm_class` 和其他 kwargs 不进 IR。缺类名时 DSPy 按 `dspy.LM` 装回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LmConfig {
    pub model: String,
    pub model_type: String,
    pub cache: bool,
    pub num_retries: u32,
}

/// 一个 predictor 的配置：用哪张签名、引用哪些 demo、调用参数、可选 LM。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predictor {
    pub name: String,
    /// 指向 [`Signature::name`]。
    pub signature: String,
    /// 指向 [`Demo::id`]，顺序即 few-shot 顺序。
    pub demos: Vec<String>,
    pub config: PredictorConfig,
    pub lm: Option<LmConfig>,
}

/// 优化器跑完之后留在 IR 里的状态。DSPy 状态 JSON 不写这一段，所以它只存在于 IR。
///
/// `score_millis` 是指标分数的毫单位：`0.875` 写成 `875`。
/// `optimizer`、`metric`、`score_millis` 是 `Option`：缺键和 `null` 都表示没有。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizationState {
    pub optimizer: Option<String>,
    pub status: OptimizationStatus,
    pub metric: Option<String>,
    pub score_millis: Option<i64>,
    pub compiled: bool,
}

/// 优化状态。未知字符串在 serde 层拒绝，不降级成未优化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationStatus {
    Unoptimized,
    Optimized,
    Failed,
}

/// 一份 DSPy 程序的 IR。`version` 必须是 [`crate::IR_VERSION`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub version: u32,
    pub program_id: String,
    pub signatures: Vec<Signature>,
    pub demos: Vec<Demo>,
    pub predictors: Vec<Predictor>,
    pub optimization: OptimizationState,
}

impl Program {
    pub fn signature(&self, name: &str) -> Option<&Signature> {
        self.signatures.iter().find(|sig| sig.name == name)
    }

    pub fn demo(&self, id: &str) -> Option<&Demo> {
        self.demos.iter().find(|demo| demo.id == id)
    }

    /// 从签名上删除一个字段，并去掉该签名各 demo 里的同名值。
    ///
    /// DSPy `Signature.delete` 对缺席的名字执行 `fields.pop(name, None)`，不报错。
    /// 这里名字为空是 DI1001，签名不存在是 DI1002，字段不存在是 DI1004，并且不改程序。
    pub fn delete_field(&mut self, signature: &str, field: &str) -> Result<(), IrError> {
        if blank(field) {
            return Err(IrError::new(
                ErrorCode::MissingFieldName,
                "删除的字段名为空",
            ));
        }
        if blank(signature) {
            return Err(IrError::new(
                ErrorCode::EmptyIdentifier,
                "删除字段时签名名为空",
            ));
        }
        let Some(index) = self.signatures.iter().position(|sig| sig.name == signature) else {
            return Err(IrError::new(
                ErrorCode::UnknownSignature,
                format!("签名 `{signature}` 不存在"),
            ));
        };
        let sig = &mut self.signatures[index];
        let in_pos = sig.inputs.iter().position(|item| item.name == field);
        let out_pos = sig.outputs.iter().position(|item| item.name == field);
        match (in_pos, out_pos) {
            (None, None) => {
                return Err(IrError::new(
                    ErrorCode::FieldNotFound,
                    format!("签名 `{signature}` 上没有字段 `{field}`"),
                ));
            }
            (Some(_), Some(_)) => {
                return Err(IrError::new(
                    ErrorCode::DuplicateId,
                    format!("签名 `{signature}` 的输入和输出都有字段 `{field}`，拒绝删除"),
                ));
            }
            (Some(pos), None) => {
                sig.inputs.remove(pos);
            }
            (None, Some(pos)) => {
                sig.outputs.remove(pos);
            }
        }
        for demo in &mut self.demos {
            if demo.signature == signature {
                demo.values.remove(field);
            }
        }
        Ok(())
    }
}

pub(crate) fn blank(text: &str) -> bool {
    text.trim().is_empty()
}
