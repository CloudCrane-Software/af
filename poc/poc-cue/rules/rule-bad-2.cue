// 反例 2：op 不在枚举里（"==" 不允许）
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [{id: "bad-2", kind: "threshold", desc: "op 非法", params: {metric: "x", op: "==", value: 1}}]
_ruleIds:  or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
