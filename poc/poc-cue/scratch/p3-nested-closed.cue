// 探针 P3：definition 里的嵌套 struct literal 是否也 closed？
package probe

#D: {a: {b: int}}
v: #D & {a: {b: 1, c: 2}}
