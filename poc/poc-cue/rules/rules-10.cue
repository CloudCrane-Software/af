// 十条配方规则样例（阈值×2 / 比例×2 / 时间窗 / dwell / 窗口计数 / 指纹 / 升级链 / 组合）
// _ruleDefs 是原始输入（hidden，不进 export）；rules 是加约束后的导出视图。
package rules

_ruleDefs: [...#Rule]
_ruleDefs: [
	// 1 阈值：单件节拍超时 → 预警
	{id: "cycle-time-warn", kind: "threshold", desc: "单件节拍超过阈值 → 预警", params: {metric: "cycle_time_sec", op: ">", value: 120}},
	// 2 阈值：炉温临界 → 停线
	{id: "oven-temp-crit", kind: "threshold", desc: "炉温达到临界 → 停线", params: {metric: "oven_temp_c", op: ">=", value: 85}},
	// 3 比例：缺陷率（critical 严于 warn 是 schema 里的跨字段约束）
	{id: "defect-ratio", kind: "ratio", desc: "班次缺陷率预警/升级", params: {numerator: "ng_count", denominator: "total_count", warn: 0.02, critical: 0.05}},
	// 4 比例：返修率
	{id: "rework-ratio", kind: "ratio", desc: "返修率超限", params: {numerator: "rework_count", denominator: "total_count", warn: 0.01, critical: 0.03}},
	// 5 时间窗：夜班静默（跨零点；"now 在窗内"留给运行时，见 RESULT）
	{id: "night-quiet", kind: "timeWindow", desc: "夜班静默 22:00-06:00", params: {start: "22:00", end: "06:00"}},
	// 6 组合：静默时段的失败突增直接 page（引用第 8、5 条 —— 声明式无序，写在前面也行）
	{id: "burst-in-quiet", kind: "combo", desc: "静默时段失败突增 → 直接 page", params: {allOf: ["fail-burst", "night-quiet"]}},
	// 7 连续越限（dwell）：温度连续 3 个采样 >80 → 升级
	{id: "temp-dwell-3", kind: "dwell", desc: "温度连续越限 → 升级", params: {metric: "oven_temp_c", op: ">", value: 80, samples: 3}},
	// 8 窗口计数：10 分钟内 ≥3 次失败
	{id: "fail-burst", kind: "count", desc: "失败突增", params: {metric: "unit_fail", op: ">=", atLeast: 3, within: "10m"}},
	// 9 指纹描述：告警折叠的指纹键 + 折叠时长
	{id: "alarm-fingerprint", kind: "fingerprint", desc: "同工位同错误码折叠 30 分钟", params: {fields: ["station", "error_code"], ttl: "30m"}},
	// 10 升级链参数：5m→班组长，15m→值班经理(page)，30m→停线（严格递增是 schema 静态约束）
	{id: "escalation-chain", kind: "escalation", desc: "安灯升级链", params: {chain: [
		{afterMs: 300000, notify: ["班组长"], action: "notify"},
		{afterMs: 900000, notify: ["值班经理"], action: "page"},
		{afterMs: 1800000, notify: ["设备科"], action: "stop-line"},
	]}},
]

// 引用完整性：combo.allOf 必须 ⊆ 已声明规则 id（or() 把 id 集转成析取）
_ruleIds: or([for r in _ruleDefs {r.id}])
rules: [for r in _ruleDefs {
	r
	if r.kind == "combo" {params: allOf: [..._ruleIds]}
}]
