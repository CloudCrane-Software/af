package probe

#B: {kind: string, id: string}
// 写法1： & 组合
#T1: #B & {kind: "x", params: int}
v1: #T1 & {id: "a", kind: "x", params: 1}
// 写法2： embedding 组合
#T2: {#B, kind: "x", params: int}
v2: #T2 & {id: "b", kind: "x", params: 2}
