// 反例（探针）：安灯 trigger 引用了不存在的评估函数 —— 引用完整性必须静态拒绝
package line

lineBadRef: #Line & {
	id:          "line-bad-ref"
	displayName: "悬空引用坏例子"
	graphTemplates: {
		main: {templateId: "assembling-v3", version: "1.0.0", entry: "start"}
	}
	evalFunctions: {
		cycleTimeSla: {name: "cycleTimeSla", kind: "threshold", timeoutMs: 2000}
	}
	tools: {
		plcGate: {name: "plcGate", driver: "opcua", endpoint: "opc.tcp://10.1.0.21:4840"}
	}
	andonRules: [
		{id: "andon-1", trigger: "pressureSla", severity: "warn", notify: ["班组长"]}, // ← 没有这个评估函数
	]
	artifactSchema: {
		kind: "unit"
		fields: {sn: {type: "string"}}
		required: ["sn"]
	}
}
