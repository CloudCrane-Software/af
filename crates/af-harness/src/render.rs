//! 最小占位替换。`{{name}}` 和 `{{ name }}` 换成注入值。
//!
//! 替换是字面的，不递归展开，不执行表达式。未声明的槽、声明了但没注入的槽，都拒绝。

use crate::error::{Error, Result};
use crate::spec::{placeholders, ContextTemplate};
use std::collections::BTreeMap;

/// 渲染系统提示。`vars` 的键是槽名。
pub fn render(template: &ContextTemplate, vars: &BTreeMap<String, String>) -> Result<String> {
    for slot in &template.slots {
        if !vars.contains_key(slot) {
            return Err(Error::MissingSlot(slot.clone()));
        }
    }
    let mut out = String::with_capacity(template.system.len());
    let mut rest = template.system.as_str();
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::InvalidSpec("模板有未闭合的 {{".into()));
        };
        let name = after[..end].trim();
        if !template.slots.iter().any(|slot| slot == name) {
            return Err(Error::UnknownSlot(name.to_string()));
        }
        let value = vars
            .get(name)
            .ok_or_else(|| Error::MissingSlot(name.to_string()))?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    // 声明过、正文里没出现的槽已经在上面要求注入，这里只确认占位符都能被抽出。
    let _ = placeholders(&template.system)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{decode_spec, ContextTemplate};

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn replaces_slots_literally_and_does_not_reexpand() {
        let spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        let text = render(
            &spec.context_template,
            &vars(&[("task", "发货 {{artifacts}}"), ("artifacts", "无")]),
        )
        .unwrap();
        assert_eq!(
            text,
            "你是产线执行器。任务：发货 {{artifacts}}\n已有工件：无"
        );
    }

    #[test]
    fn missing_slot_value_is_rejected() {
        let template = ContextTemplate {
            system: "任务 {{task}}".into(),
            slots: vec!["task".into()],
        };
        let err = render(&template, &BTreeMap::new()).unwrap_err();
        assert!(matches!(err, Error::MissingSlot(ref name) if name == "task"));
    }

    #[test]
    fn unknown_placeholder_is_rejected_at_spec_validation() {
        let mut spec = crate::spec::decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        spec.context_template = ContextTemplate {
            system: "{{task}} {{nope}}".into(),
            slots: vec!["task".into()],
        };
        let err = spec.validate().unwrap_err();
        assert!(matches!(err, Error::UnknownSlot(ref name) if name == "nope"));
    }
}
