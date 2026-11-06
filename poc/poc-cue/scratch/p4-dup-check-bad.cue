// 探针 P4（坏例）：有重复时同一惯用法应报错（false & true 冲突）
package probe

xs: ["a", "b", "a"]
dupCheck: [for i, x in xs for j, y in xs if i < j if x == y {false}]
dupCheck: [...true]
