// 反例 4：升级链 afterMs 非严格递增（schema 内检查式 comprehension 必须拒绝）
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [{id: "bad-4", kind: "escalation", desc: "afterMs 非递增", params: {chain: [
	{afterMs: 900000, notify: ["a"], action: "page"},
	{afterMs: 300000, notify: ["b"], action: "notify"},
]}}]
_ruleIds:  or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
