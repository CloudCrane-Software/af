// 合法产线实例：cue vet line/line.cue line/line-a.cue 必须通过
package line

lineA: #Line & {
	id:          "line-a"
	displayName: "总装 A 线（PoC 示例）"

	graphTemplates: {
		main: {templateId: "assembling-v3", version: "1.4.2", entry: "start"}
		pack: {templateId: "packing-v1", version: "0.9.0", entry: "intake"}
	}
	evalFunctions: {
		cycleTimeSla: {name: "cycleTimeSla", kind: "threshold", timeoutMs: 2000}
		defectRatio:  {name: "defectRatio", kind: "ratio", timeoutMs: 3000}
		tempDwell:    {name: "tempDwell", kind: "dwell", timeoutMs: 1500}
	}
	tools: {
		plcGate: {name: "plcGate", driver: "opcua", endpoint: "opc.tcp://10.1.0.21:4840"}
		vision:  {name: "vision", driver: "http", endpoint: "http://10.1.0.22:8080"}
		printer: {name: "printer", driver: "modbus", endpoint: "/dev/ttyUSB0:115200"}
	}
	andonRules: [
		{id: "andon-1", trigger: "cycleTimeSla", severity: "warn", notify: ["班组长"], escalateAfterMs: 300000, escalateTo: ["值班经理"]},
		{id: "andon-2", trigger: "tempDwell", severity: "critical", notify: ["班组长", "设备科"]},
	]
	artifactSchema: {
		kind: "unit"
		fields: {
			sn:      {type: "string"}
			qcScore: {type: "number"}
			grade:   {type: "enum", enumValues: ["A", "B", "C"]}
			builtAt: {type: "timestamp"}
		}
		required: ["sn", "builtAt"]
	}
}
