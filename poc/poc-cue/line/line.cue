// =============================================================================
// 产线定义 schema —— 底座随发行维护；运营者空间（L4）只读引用，不可改这里。
// #Line 是 definition ⇒ closed：实例出现未声明字段 = cue vet 拒绝（未知字段拒绝）。
// 集合类字段（键开放）写成 [string]: #X：键可以增，值的形状仍然 closed。
// =============================================================================
package line


// ---- ① 图模板引用 ----
#GraphTemplateRef: {
	templateId: string                          // 图库模板 ID
	version:    string & =~#"^\d+\.\d+\.\d+$"#  // 语义版本
	entry:      string                          // 模板内入口节点名
}

// ---- ② 评估函数集 ----
#EvalFunction: {
	name:      string
	kind:      "threshold" | "ratio" | "timeWindow" | "dwell" | "count" | "fingerprint" | "escalation" | "combo"
	timeoutMs: int & >0 & <=10_000
}

// ---- ③ 工具集 ----
#Tool: {
	name:     string
	driver:   "http" | "mqtt" | "opcua" | "modbus"
	endpoint: string
}

// ---- ④ 安灯规则 ----
#AndonRule: {
	id:       string & =~#"^andon-[a-z0-9-]+$"#
	severity: "info" | "warn" | "critical"
	notify:   [string, ...string] // 非空：至少一个通知目标
	// 升级链参数：超时未响应转交谁
	escalateAfterMs: int & >=0 | *0
	escalateTo?:     [...string]
	// trigger 不在这里定死：在 #Line 里收紧为"必须是已声明的评估函数名"
	trigger: string
}

// ---- ⑤ 工件 schema（产线产出物的元 schema） ----
#ArtifactField: {
	type: "string" | "number" | "bool" | "timestamp" | "enum"
	// enum 类型必须给非空取值表（联动约束，需 vet -c 落地，见 RESULT）
	if type == "enum" {enumValues: [string, ...string]}
}

// ---- 产线 ----
#Line: {
	id:          string & =~#"^[a-z][a-z0-9-]*$"#
	displayName: string

	// ① 图模板引用：按别名索引；键开放（可加模板），值 closed
	graphTemplates: [string]: #GraphTemplateRef
	// ② 评估函数集（空集允许，但此时任何安灯规则都因 or([]) 无从引用而无法通过 vet）
	evalFunctions: [string]: #EvalFunction
	// ③ 工具集
	tools: [string]: #Tool

	// ④ 安灯规则 + 引用完整性①：trigger ∈ evalFunctions 的键
	//    （or() 把 map 键集转成析取，引用完整性变成静态可查；
	//     必须内联进约束 —— 若落成独立 hidden 字段，未解析析取会被 vet 视为 incomplete）
	andonRules: [...#AndonRule & {trigger: or([for k, _ in evalFunctions {k}])}]

	// ⑤ 工件 schema + 引用完整性②：required ⊆ fields 的键
	artifactSchema: {
		kind:     string
		fields:   [string]: #ArtifactField
		required: [...or([for k, _ in fields {k}])]
	}
}
