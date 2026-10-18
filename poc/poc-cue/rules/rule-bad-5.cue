// 反例 5：未知 kind（不在析取里 ⇒ 整条规则无分支可匹配）
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [{id: "bad-5", kind: "quantum", desc: "kind 不存在"}]
_ruleIds:  or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
