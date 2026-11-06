// =============================================================================
// L3 发行默认配方（随产品发布）。
// 纪律（本次 PoC 的核心发现）：每个希望可被 L4 覆盖的字段必须写成 *默认值 | 类型约束。
// 写成裸具体值时，L4 的同名改动是"冲突报错"而不是"覆盖"（见 pit-concrete-defaults.cue）。
// =============================================================================
package recipe

scheduling: {
	batchIntervalSec:  *30 | int & >0
	maxConcurrentJobs: *4 | int & >=1 & <=16
	retry: {
		maxAttempts: *3 | int & >=0 & <=10
		backoffSec:  *5 | int & >0
	}
}
thresholds: {
	cycleTimeWarnSec: *120 | int & >0
	defectRatioWarn:  *0.02 | number & >0 & <1
	temperatureCritC: *85 | int & >0
}
