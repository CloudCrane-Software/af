// L4 运营者覆盖：只改两个字段。约束（类型/上下界）来自 L3，L4 改破约束会被拒绝。
package recipe

scheduling: batchIntervalSec: 15
thresholds: defectRatioWarn:  0.05
