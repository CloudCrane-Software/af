//! v1 字段清单。规范样例文档里每个键的 JSON 指针都在这里。
//!
//! 指针用规范样例的下标 0。动态 map `demos[].values` 的键是签名字段名，
//! 不是 schema 键，所以清单停在 `values` 本身。

/// 清单上的一个键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// 逻辑路径，写入 RESULT 的字段清单用这一列。
    pub name: &'static str,
    /// 规范样例中的 JSON 指针。
    pub pointer: &'static str,
    /// `Option` 字段。缺键和 `null` 都是未设置；反例是错误的 JSON 类型。
    pub optional: bool,
}

/// schema v1 的全部键。测试对每一条做「样例里有这个键」和「删掉这个键就拒绝」。
pub const V1_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        name: "version",
        pointer: "/version",
        optional: false,
    },
    FieldSpec {
        name: "program_id",
        pointer: "/program_id",
        optional: false,
    },
    FieldSpec {
        name: "signatures",
        pointer: "/signatures",
        optional: false,
    },
    FieldSpec {
        name: "signatures.name",
        pointer: "/signatures/0/name",
        optional: false,
    },
    FieldSpec {
        name: "signatures.instructions",
        pointer: "/signatures/0/instructions",
        optional: false,
    },
    FieldSpec {
        name: "signatures.inputs",
        pointer: "/signatures/0/inputs",
        optional: false,
    },
    FieldSpec {
        name: "signatures.inputs.name",
        pointer: "/signatures/0/inputs/0/name",
        optional: false,
    },
    FieldSpec {
        name: "signatures.inputs.prefix",
        pointer: "/signatures/0/inputs/0/prefix",
        optional: false,
    },
    FieldSpec {
        name: "signatures.inputs.description",
        pointer: "/signatures/0/inputs/0/description",
        optional: false,
    },
    FieldSpec {
        name: "signatures.inputs.type_name",
        pointer: "/signatures/0/inputs/0/type_name",
        optional: false,
    },
    FieldSpec {
        name: "signatures.outputs",
        pointer: "/signatures/0/outputs",
        optional: false,
    },
    FieldSpec {
        name: "signatures.outputs.name",
        pointer: "/signatures/0/outputs/0/name",
        optional: false,
    },
    FieldSpec {
        name: "signatures.outputs.prefix",
        pointer: "/signatures/0/outputs/0/prefix",
        optional: false,
    },
    FieldSpec {
        name: "signatures.outputs.description",
        pointer: "/signatures/0/outputs/0/description",
        optional: false,
    },
    FieldSpec {
        name: "signatures.outputs.type_name",
        pointer: "/signatures/0/outputs/0/type_name",
        optional: false,
    },
    FieldSpec {
        name: "demos",
        pointer: "/demos",
        optional: false,
    },
    FieldSpec {
        name: "demos.id",
        pointer: "/demos/0/id",
        optional: false,
    },
    FieldSpec {
        name: "demos.signature",
        pointer: "/demos/0/signature",
        optional: false,
    },
    FieldSpec {
        name: "demos.values",
        pointer: "/demos/0/values",
        optional: false,
    },
    FieldSpec {
        name: "predictors",
        pointer: "/predictors",
        optional: false,
    },
    FieldSpec {
        name: "predictors.name",
        pointer: "/predictors/0/name",
        optional: false,
    },
    FieldSpec {
        name: "predictors.signature",
        pointer: "/predictors/0/signature",
        optional: false,
    },
    FieldSpec {
        name: "predictors.demos",
        pointer: "/predictors/0/demos",
        optional: false,
    },
    FieldSpec {
        name: "predictors.config",
        pointer: "/predictors/0/config",
        optional: false,
    },
    FieldSpec {
        name: "predictors.config.temperature_millis",
        pointer: "/predictors/0/config/temperature_millis",
        optional: true,
    },
    FieldSpec {
        name: "predictors.config.rollout_id",
        pointer: "/predictors/0/config/rollout_id",
        optional: true,
    },
    FieldSpec {
        name: "predictors.config.n",
        pointer: "/predictors/0/config/n",
        optional: true,
    },
    FieldSpec {
        name: "predictors.lm",
        pointer: "/predictors/0/lm",
        optional: true,
    },
    FieldSpec {
        name: "predictors.lm.model",
        pointer: "/predictors/0/lm/model",
        optional: false,
    },
    FieldSpec {
        name: "predictors.lm.model_type",
        pointer: "/predictors/0/lm/model_type",
        optional: false,
    },
    FieldSpec {
        name: "predictors.lm.cache",
        pointer: "/predictors/0/lm/cache",
        optional: false,
    },
    FieldSpec {
        name: "predictors.lm.num_retries",
        pointer: "/predictors/0/lm/num_retries",
        optional: false,
    },
    FieldSpec {
        name: "optimization",
        pointer: "/optimization",
        optional: false,
    },
    FieldSpec {
        name: "optimization.optimizer",
        pointer: "/optimization/optimizer",
        optional: true,
    },
    FieldSpec {
        name: "optimization.status",
        pointer: "/optimization/status",
        optional: false,
    },
    FieldSpec {
        name: "optimization.metric",
        pointer: "/optimization/metric",
        optional: true,
    },
    FieldSpec {
        name: "optimization.score_millis",
        pointer: "/optimization/score_millis",
        optional: true,
    },
    FieldSpec {
        name: "optimization.compiled",
        pointer: "/optimization/compiled",
        optional: false,
    },
];
