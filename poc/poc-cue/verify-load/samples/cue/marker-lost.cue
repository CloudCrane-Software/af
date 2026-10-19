// 坏样例 E9001-b：写了具体值但忘了 * 标记（30 | int 中裸 30 被析取吸收，marker 丢失）
// 与 marker-missing 同症状：无覆盖时 incomplete —— * 标记是唯一能保住默认的手段
package marker

batchIntervalSec: 30 | int & >0
