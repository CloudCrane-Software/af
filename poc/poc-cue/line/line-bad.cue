// 反例：比合法实例多一个未声明字段 owner —— cue vet 必须拒绝（未知字段拒绝）
package line

lineBad: #Line & {
	id:          "line-bad"
	displayName: "坏例子"
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
		{id: "andon-1", trigger: "cycleTimeSla", severity: "warn", notify: ["班组长"]},
	]
	artifactSchema: {
		kind: "unit"
		fields: {sn: {type: "string"}}
		required: ["sn"]
	}
	owner: "张三" // ← 未声明字段，#Line closed ⇒ 必须拒绝
}
