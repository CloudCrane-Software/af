//! IR 校验与信封错误。码从 DI1001 起，稳定给适配器日志和测试用。

use std::fmt;

/// 稳定错误码。数字只增不改。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// 签名字段或 demo 值的字段名为空。
    MissingFieldName,
    /// demo、predictor 或删除操作指向的签名不存在。
    UnknownSignature,
    /// predictor 指向的 demo 不存在。
    UnknownDemo,
    /// 删除的字段不在该签名上。DSPy 这里静默 no-op，本 crate 必须报错。
    FieldNotFound,
    /// 文档不是对象，或对象里没有 `version`。
    MissingVersion,
    /// `version` 存在，但不是整数 1。
    UnknownVersion,
    /// 程序、签名、demo、predictor 的标识为空。
    EmptyIdentifier,
    /// 同一作用域里标识或字段名重复。
    DuplicateId,
    /// demo 的值用了签名表里没有的字段名。
    UnknownField,
    /// 存档的 signature.fields 条数与签名表对不上，无法按位置补字段名。
    ArchiveFieldMismatch,
    /// 值在类型上能读入，但违反取值约束。
    InvalidValue,
    /// demo 没覆盖签名表里的某个字段。
    IncompleteDemo,
    /// predictor 引用的 demo 属于另一张签名。
    DemoSignatureMismatch,
    /// JSON 类型、未知键或存档形状不对。
    Schema,
    /// 存档里的 predictor 名在 IR 里不存在。
    UnknownPredictor,
    /// IR 里的 predictor 在存档里没有对应条目。
    ArchivePredictorMissing,
    /// 存档字段自带的名字与签名表该位置的名字不一致。
    ArchiveFieldNameConflict,
    /// 存档 demo 条数与 predictor 的 demo 引用条数不一致。
    ArchiveDemoCount,
    /// 两个 predictor 共用一张签名，但存档给出的指令或字段文本不一致。
    ArchiveSignatureConflict,
    /// v1 对象里缺少必填键。
    MissingField,
    /// 多个 predictor 共用一条 demo，但存档里的值不一致。
    ArchiveDemoConflict,
}

impl ErrorCode {
    /// 对外稳定的 `DI1xxx` 字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingFieldName => "DI1001",
            Self::UnknownSignature => "DI1002",
            Self::UnknownDemo => "DI1003",
            Self::FieldNotFound => "DI1004",
            Self::MissingVersion => "DI1005",
            Self::UnknownVersion => "DI1006",
            Self::EmptyIdentifier => "DI1007",
            Self::DuplicateId => "DI1008",
            Self::UnknownField => "DI1009",
            Self::ArchiveFieldMismatch => "DI1010",
            Self::InvalidValue => "DI1011",
            Self::IncompleteDemo => "DI1012",
            Self::DemoSignatureMismatch => "DI1013",
            Self::Schema => "DI1014",
            Self::UnknownPredictor => "DI1015",
            Self::ArchivePredictorMissing => "DI1016",
            Self::ArchiveFieldNameConflict => "DI1017",
            Self::ArchiveDemoCount => "DI1018",
            Self::ArchiveSignatureConflict => "DI1019",
            Self::MissingField => "DI1020",
            Self::ArchiveDemoConflict => "DI1021",
        }
    }

    /// 大类名，给结构化日志用。
    pub const fn name(self) -> &'static str {
        match self {
            Self::MissingFieldName => "MISSING_FIELD_NAME",
            Self::UnknownSignature => "UNKNOWN_SIGNATURE",
            Self::UnknownDemo => "UNKNOWN_DEMO",
            Self::FieldNotFound => "FIELD_NOT_FOUND",
            Self::MissingVersion => "MISSING_VERSION",
            Self::UnknownVersion => "UNKNOWN_VERSION",
            Self::EmptyIdentifier => "EMPTY_IDENTIFIER",
            Self::DuplicateId => "DUPLICATE_ID",
            Self::UnknownField => "UNKNOWN_FIELD",
            Self::ArchiveFieldMismatch => "ARCHIVE_FIELD_MISMATCH",
            Self::InvalidValue => "INVALID_VALUE",
            Self::IncompleteDemo => "INCOMPLETE_DEMO",
            Self::DemoSignatureMismatch => "DEMO_SIGNATURE_MISMATCH",
            Self::Schema => "SCHEMA",
            Self::UnknownPredictor => "UNKNOWN_PREDICTOR",
            Self::ArchivePredictorMissing => "ARCHIVE_PREDICTOR_MISSING",
            Self::ArchiveFieldNameConflict => "ARCHIVE_FIELD_NAME_CONFLICT",
            Self::ArchiveDemoCount => "ARCHIVE_DEMO_COUNT",
            Self::ArchiveSignatureConflict => "ARCHIVE_SIGNATURE_CONFLICT",
            Self::MissingField => "MISSING_FIELD",
            Self::ArchiveDemoConflict => "ARCHIVE_DEMO_CONFLICT",
        }
    }

    /// 按码值顺序的全表。测试用来锁住 DI1001 起连续分配。
    pub const fn all() -> &'static [Self] {
        &[
            Self::MissingFieldName,
            Self::UnknownSignature,
            Self::UnknownDemo,
            Self::FieldNotFound,
            Self::MissingVersion,
            Self::UnknownVersion,
            Self::EmptyIdentifier,
            Self::DuplicateId,
            Self::UnknownField,
            Self::ArchiveFieldMismatch,
            Self::InvalidValue,
            Self::IncompleteDemo,
            Self::DemoSignatureMismatch,
            Self::Schema,
            Self::UnknownPredictor,
            Self::ArchivePredictorMissing,
            Self::ArchiveFieldNameConflict,
            Self::ArchiveDemoCount,
            Self::ArchiveSignatureConflict,
            Self::MissingField,
            Self::ArchiveDemoConflict,
        ]
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 一条 IR 错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct IrError {
    pub code: ErrorCode,
    pub message: String,
}

impl IrError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }

    /// 大类名，给结构化日志用。
    pub fn kind_name(&self) -> &'static str {
        self.code.name()
    }
}

/// 一次解析或校验的全部错误。信封错误通常只有一条；语义校验会收齐再返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrFailure {
    pub errors: Vec<IrError>,
}

impl IrFailure {
    pub fn one(error: IrError) -> Self {
        Self {
            errors: vec![error],
        }
    }

    pub fn contains(&self, code: ErrorCode) -> bool {
        self.errors.iter().any(|err| err.code == code)
    }
}

impl fmt::Display for IrFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, err) in self.errors.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{err}")?;
        }
        Ok(())
    }
}

impl std::error::Error for IrFailure {}
