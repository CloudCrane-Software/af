// =============================================================================
// 配方规则 DSL —— 规则 = kind 判别 + per-kind closed params。
// #Rule 是 8 个 kind 定义的析取：unify 时只有一个分支能匹配 ⇒ 参数形状/kind 强绑定。
// 全部定义 closed：params 里多一个未声明字段即被拒。
// =============================================================================
package rules

#Duration: string & =~#"^\d+(ms|s|m|h)$"#

// ---- per-kind params（全部 closed）----
#ThresholdParams: {
	metric: string
	op:     ">" | ">=" | "<" | "<="
	value:  number
}
#RatioParams: {
	numerator:   string
	denominator: string
	warn:        number & >0 & <1
	critical:    number & >0 & <1 & >warn // 跨字段：critical 必须严于 warn
}
#TimeWindowParams: {
	start: string & =~#"^\d{2}:\d{2}$"#
	end:   string & =~#"^\d{2}:\d{2}$"# & !=start
	// 注意："now 是否在窗内/跨零点窗口"属于运行时求值，CUE 只能校验参数形状（见 RESULT）
}
#DwellParams: {
	metric:  string
	op:      ">" | ">="
	value:   number
	samples: int & >=2 // 连续 N 个采样越限才触发
}
#CountParams: {
	metric:  string
	op:      ">=" | ">"
	atLeast: int & >=1
	within:  #Duration
}
#FingerprintParams: {
	fields: [string, ...string] // 指纹键字段，非空
	ttl:    #Duration
}
#EscalateStep: {
	afterMs: int & >0
	notify:  [string, ...string]
	action:  "notify" | "page" | "stop-line"
}
#EscalationParams: {
	chain: [#EscalateStep, ...#EscalateStep] // 非空
	// 静态可查：链必须按 afterMs 严格递增。
	// 手法 = 检查式 comprehension 吐 bool + 与 [...true] unify：吐出 false 即冲突报错
	_check: [for i, r in chain if i > 0 {r.afterMs > chain[i-1].afterMs}]
	_check: [...true]
}
#ComboParams: {
	allOf: [string, ...string] // 引用同配方其它规则 id（存在性在实例层用 or() 收紧）
}

// ---- 规则本体：kind 析取 ----
// 注意（v0.17 实测）：closed definition 不能用 `#RuleBase & {新字段}` 扩展（field not allowed），
// 必须用 embedding 组合 {#RuleBase, ...} —— 见 RESULT 踩坑记录
#RuleBase: {
	id:      string & =~#"^[a-z][a-z0-9-]*$"#
	kind:    string
	desc:    string
	enabled: *true | bool
}
#ThresholdRule:   {#RuleBase, kind: "threshold", params: #ThresholdParams}
#RatioRule:       {#RuleBase, kind: "ratio", params: #RatioParams}
#TimeWindowRule:  {#RuleBase, kind: "timeWindow", params: #TimeWindowParams}
#DwellRule:       {#RuleBase, kind: "dwell", params: #DwellParams}
#CountRule:       {#RuleBase, kind: "count", params: #CountParams}
#FingerprintRule: {#RuleBase, kind: "fingerprint", params: #FingerprintParams}
#EscalationRule:  {#RuleBase, kind: "escalation", params: #EscalationParams}
#ComboRule:       {#RuleBase, kind: "combo", params: #ComboParams}

#Rule: #ThresholdRule | #RatioRule | #TimeWindowRule | #DwellRule | #CountRule | #FingerprintRule | #EscalationRule | #ComboRule
