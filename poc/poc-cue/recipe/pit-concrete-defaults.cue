// 反面教材：L3 若把默认值写成裸具体值（没有 default 标记），
// L4 的同名改动会变成 unify 冲突而不是覆盖 —— 这是 CUE 与"后写赢"配置系统最大的语义差异。
package recipe

scheduling: batchIntervalSec: 30 // 裸具体值
thresholds: defectRatioWarn:  0.02
