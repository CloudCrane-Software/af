// 自动生成：10 条规则（gen_rules.py），schema 见 ../../rules/rules.cue
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [
	{id: "rule-0", kind: "threshold", desc: "gen threshold #0", params: {metric: "m0_sec", op: ">", value: 100}},
	{id: "rule-1", kind: "ratio", desc: "gen ratio #1", params: {numerator: "ng_count", denominator: "total_count", warn: 0.015, critical: 0.03}},
	{id: "rule-2", kind: "timeWindow", desc: "gen timeWindow #2", params: {start: "22:00", end: "06:00"}},
	{id: "rule-3", kind: "dwell", desc: "gen dwell #3", params: {metric: "m3_c", op: ">", value: 63, samples: 3}},
	{id: "rule-4", kind: "count", desc: "gen count #4", params: {metric: "unit_fail", op: ">=", atLeast: 3, within: "10m"}},
	{id: "rule-5", kind: "fingerprint", desc: "gen fingerprint #5", params: {fields: ["station", "error_code"], ttl: "30m"}},
	{id: "rule-6", kind: "escalation", desc: "gen escalation #6", params: {chain: [{afterMs: 300000, notify: ["班组长"], action: "notify"}, {afterMs: 900000, notify: ["值班经理"], action: "page"}]}},
	{id: "rule-7", kind: "combo", desc: "gen combo #7", params: {allOf: ["rule-0"]}},
	{id: "rule-8", kind: "threshold", desc: "gen threshold #8", params: {metric: "m8_sec", op: ">", value: 108}},
	{id: "rule-9", kind: "ratio", desc: "gen ratio #9", params: {numerator: "ng_count", denominator: "total_count", warn: 0.030, critical: 0.06}},
]

// 与 rules-10.cue 相同的导出视图 + 引用完整性
_ruleIds: or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
