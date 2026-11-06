//! Cedar 角色策略：谁（Role）可以对哪个资源（Line/命名空间）执行哪个工具（Action）。

use anyhow::Result;
use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityId, EntityTypeName, EntityUid, PolicySet,
    Request,
};

/// Cedar 策略源。词表在策略文本里固定：
/// - 实体类型：Role（主体角色）、Line（生产资源/命名空间）、Action（工具）
/// - p1：Reviewer 对 line-a 只放行工具 test
/// - p2：builder 对 line-a 只放行工具 git（“只允许 git”的一条）
///
/// 说明：这里是 schema-less 用法（Request::new 的 schema 传 None）。没有 schema 时
/// Cedar 不校验实体类型/属性的词表——这是对“词表绑定”适配度的关键观察，见 RESULT.md。
pub const CEDAR_POLICIES: &str = r#"
permit(principal == Role::"reviewer", action == Action::"test", resource == Line::"line-a");
permit(principal == Role::"builder", action == Action::"git", resource == Line::"line-a");
"#;

/// 判定：角色 role 是否允许在命名空间 namespace 上执行工具 tool。
pub fn cedar_allows(role: &str, tool: &str, namespace: &str) -> Result<bool> {
    let policies: PolicySet = CEDAR_POLICIES.parse()?;
    let entities = Entities::empty();
    let request = Request::new(
        uid("Role", role),
        uid("Action", tool),
        uid("Line", namespace),
        Context::empty(),
        None, // schema-less：不校验词表
    )?;
    let response = Authorizer::new().is_authorized(&request, &policies, &entities);
    Ok(response.decision() == Decision::Allow)
}

fn uid(type_name: &str, id: &str) -> EntityUid {
    EntityUid::from_type_name_and_id(
        EntityTypeName::from_str(type_name).expect("valid type name"),
        EntityId::from_str(id).expect("valid entity id"),
    )
}

use std::str::FromStr;
