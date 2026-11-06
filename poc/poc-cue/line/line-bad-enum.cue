// 反例（探针）：工件字段 type=enum 却不给 enumValues 取值表
// 预期：普通 cue vet 可能放过（联动约束产生的是"不完整"值），cue vet -c 必须拒绝
package line

lineBadEnum: #Line & {
	id:          "line-bad-enum"
	displayName: "枚举缺取值表"
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
		kind:    "unit"
		fields: {grade: {type: "enum"}} // ← 缺 enumValues
		required: []
	}
}
