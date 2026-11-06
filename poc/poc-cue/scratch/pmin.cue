// 最小复现：definition 内 MinFields 对 pattern-only map 是否急切求值为 false
#D: {
	m: [string]: int
	_c: struct.MinFields(m, 1)
	_c: true
}
ok: #D & {m: {a: 1}}
