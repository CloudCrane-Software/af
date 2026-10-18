// 反例 1：params 出现未声明字段 unit（threshold params closed ⇒ 拒绝）
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [{id: "bad-1", kind: "threshold", desc: "params 多了未声明字段", params: {metric: "x", op: ">", value: 1, unit: "s"}}]
_ruleIds:  or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
