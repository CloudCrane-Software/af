// 探针 P5：list 内建（UniqueItems / Contains）是否存在、自引用约束是否可行
package probe

import "list"

a: list.UniqueItems([1, 1, 2])
b: list.Contains(["a"], "a")
// 自引用写法：用第二个变量桥接
xs:  ["a", "b", "a"]
xs2: list.UniqueItems(xs)
xs2: xs
