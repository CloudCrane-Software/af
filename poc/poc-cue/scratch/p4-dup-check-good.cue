// 探针 P4（好例）：列表去重的"检查式 comprehension"惯用法 —— 无重复时应通过
package probe

xs: ["a", "b", "c"]
dupCheck: [for i, x in xs for j, y in xs if i < j if x == y {false}]
dupCheck: [...true]
