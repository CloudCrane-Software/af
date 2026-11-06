// 正确形态：可覆盖字段 = *默认值 | 类型约束（marker 可见）
package marker

batchIntervalSec: *30 | int & >0
