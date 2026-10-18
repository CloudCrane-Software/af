// 反例 3：比例规则 critical(0.02) < warn(0.05)，破坏跨字段约束
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [{id: "bad-3", kind: "ratio", desc: "critical 必须严于 warn", params: {numerator: "a", denominator: "b", warn: 0.05, critical: 0.02}}]
_ruleIds:  or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
