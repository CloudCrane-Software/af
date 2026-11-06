// 坏样例 E9001-a：可覆盖字段完全没写默认（连 *默认 | 都没有，只剩类型约束）
// 症状：无覆盖时 cue export 判 incomplete，直接装载失败
package marker

batchIntervalSec: int & >0
