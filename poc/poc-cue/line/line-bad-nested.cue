// 反例（探针）：未知字段出现在嵌套的 #Tool 值里 —— 验证 closed 是否传递到嵌套层
package line

lineBadNested: #Line & {
	id:          "line-bad-nested"
	displayName: "嵌套坏例子"
	graphTemplates: {
		main: {templateId: "assembling-v3", version: "1.0.0", entry: "start"}
	}
	evalFunctions: {
		cycleTimeSla: {name: "cycleTimeSla", kind: "threshold", timeoutMs: 2000}
	}
	tools: {
		plcGate: {name: "plcGate", driver: "opcua", endpoint: "opc.tcp://10.1.0.21:4840", timeoutSec: 5} // ← #Tool 未声明 timeoutSec
	}
	andonRules: [
		{id: "andon-1", trigger: "cycleTimeSla", severity: "warn", notify: ["班组长"]},
	]
	artifactSchema: {
		kind: "unit"
		fields: {sn: {type: "string"}}
		required: ["sn"]
	}
}
