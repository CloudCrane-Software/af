// 探针 P7：空 map 上做 or([for k,_ in m {k}]) → or([]) 是否为 bottom（schema 单独 vet 的风险点）
package probe

m: {}
names: or([for k, _ in m {k}])
v: "x" & names
