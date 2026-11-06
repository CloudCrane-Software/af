# PoC·CUE 产线定义与配方（poc-cue）RESULT

## ① 结论

**通过**。三项验证全部完成并全部符合预期：closed schema 未知字段拒绝、L4 覆盖 L3、十条配方规则静态校验（含 5 类反例全部拒绝）。CUE（cue v0.17.1 CLI）可以同时承担产线定义 schema 与配方 DSL，前提是把职责切清楚：**CUE 管"结构与参数的静态校验 + 默认/覆盖合并"，数据相关的运行时谓词交给受限表达式引擎（建议 CEL）**。运行时求值需经外部进程（实测本机 ~7-17 ms/次）或部署期预编译成 JSON（推荐，运行时零 CUE 依赖）。

环境：cue v0.17.1 静态二进制（24 MB）下载至 `af/poc/poc-cue/bin/cue`（GitHub release，linux/amd64）。本 PoC 不涉及 cargo。

## ② 做了什么

```
poc-cue/
├── bin/cue                          # v0.17.1
├── line/                            # 任务1：产线定义 schema
│   ├── line.cue                     # #Line：图模板引用/评估函数集/工具集/安灯规则/工件 schema，closed
│   ├── line-a.cue                   # 合法实例
│   ├── line-bad.cue                 # 多一个未声明字段 owner（ask 指定反例）
│   ├── line-bad-nested.cue          # 探针：嵌套层未知字段
│   ├── line-bad-ref.cue             # 探针：安灯 trigger 悬空引用
│   └── line-bad-enum.cue            # 探针：enum 缺取值表（plain vet vs -c）
├── recipe/                          # 任务2：L3/L4 覆盖语义
│   ├── recipe-default.cue           # L3 发行默认（*默认 | 类型 纪律）
│   ├── operator-override.cue        # L4 运营者只改两个字段
│   ├── pit-concrete-defaults.cue    # 反面教材：裸具体值默认 → 冲突而非覆盖
│   └── operator-override-bad.cue    # 反例：L4 改破约束（maxAttempts=99）
├── rules/                           # 任务3：配方规则 DSL
│   ├── rules.cue                    # 8 类 kind 的 closed params + kind 析取 + 递增链检查
│   ├── rules-10.cue                 # 十条规则样例
│   └── rule-bad-{1..5}.cue          # 未知字段/非法枚举/跨字段破坏/链非递增/未知 kind
└── scratch/                         # 表达力边界探针 P1-P7（覆盖语义/嵌套 closed/去重/内建）
```

## ③ 证据（命令与输出摘录）

**任务1 —— closed schema（ask 指定检查）**
```
$ bin/cue vet line/line.cue line/line-a.cue; echo $?
0
$ bin/cue vet line/line.cue line/line-bad.cue; echo $?   # 多一个未声明字段 owner
lineBad.owner: field not allowed:
    ./line/line-bad.cue:24:2
1
$ bin/cue vet line/line.cue line/line-bad-nested.cue; echo $?   # 嵌套层也 closed
lineBadNested.tools.plcGate.timeoutSec: field not allowed:      # #Tool 未声明 timeoutSec
1
$ bin/cue vet line/line.cue line/line-bad-ref.cue; echo $?      # trigger 引用完整性
lineBadRef.andonRules.0.trigger: conflicting values "cycleTimeSla" and "pressureSla":
1
```
`cue vet -c` 额外提供具体性（concreteness）门禁；line-a 通过。

**任务2 —— L4 盖 L3（ask 指定检查：cue export 合并）**
```
$ bin/cue export recipe/recipe-default.cue recipe/operator-override.cue
{ "scheduling": { "batchIntervalSec": 15, "maxConcurrentJobs": 4,
    "retry": { "maxAttempts": 3, "backoffSec": 5 } },
  "thresholds": { "cycleTimeWarnSec": 120, "defectRatioWarn": 0.05,
    "temperatureCritC": 85 } }          # 改的 15/0.05 生效，其余默认保留 ✓
$ bin/cue export recipe/pit-concrete-defaults.cue recipe/operator-override.cue
scheduling.batchIntervalSec: conflicting values 30 and 15:      # 裸具体值=冲突非覆盖
thresholds.defectRatioWarn: conflicting values 0.02 and 0.05:
1
$ bin/cue vet recipe/recipe-default.cue recipe/operator-override-bad.cue
scheduling.retry.maxAttempts: invalid value 99 (out of bound <=10):   # L4 不能破 L3 约束
1
```

**任务3 —— 十条规则与反例**
```
$ bin/cue vet rules/rules.cue rules/rules-10.cue; echo $?          # 10 条全过
0
$ bin/cue export rules/rules.cue rules/rules-10.cue | python3 -c ...
rules: 10    hidden 字段(_ruleDefs/_ruleIds)未泄漏: keys=['rules']  ✓
$ bin/cue vet rules/rules.cue rules/rule-bad-1.cue   # params 未声明字段
_ruleDefs.0.params.unit: field not allowed:
$ bin/cue vet rules/rules.cue rules/rule-bad-3.cue   # 跨字段：critical 必须严于 warn
_ruleDefs.0.params.critical: invalid value 0.02 (out of bound >0.05):
$ bin/cue vet rules/rules.cue rules/rule-bad-4.cue   # 升级链 afterMs 非递增（静态查出）
_ruleDefs.0.params._check.0: conflicting values false and true:
$ bin/cue vet rules/rules.cue rules/rule-bad-5.cue   # 未知 kind
_ruleDefs.0: 8 errors in empty disjunction:   （根因：kind "quantum" 无分支匹配）
（bad-2 op "==" 同样拒绝；5 个反例 exit 均=1）
```

**任务4 —— 运行时求值代价（本机 16 核/14G，中位 5 次）**
```
export-merge（进程冷启动+解析+合并+输出）: 7 ms
vet-line:                                  7 ms
vet-rules10:                              17 ms
二进制: 24 MB 静态
```

## ④ 对最终方案的反馈

1. **closed 语义成立且嵌套传递**：`#Line` 这类 definition 默认 closed，实例未知字段 → `field not allowed`，嵌套的 struct literal 同样 closed（探针 A3）。集合类字段（键需要开放）写 `[string]: #X`：键可增、值形状仍 closed——产线的工具集/评估函数集正需要这个语义。

2. **v0.17 坑：closed definition 不能用 `&` 扩展新字段**。`#ThresholdRule: #RuleBase & {params: ...}` 直接报 `params: field not allowed`，必须用 embedding 组合 `{#RuleBase, kind: ..., params: ...}`。网上旧教程/旧代码会踩；底座 schema 模板要按 embedding 风格写。

3. **"L4 盖 L3"不是免费的，必须靠 default 标记实现**：CUE 对同字段两个具体值是**冲突报错**而非后写覆盖（pit 实证：`conflicting values 30 and 15`）。发行默认必须写成 `*30 | int & >0` 形态——默认值 + 开放类型约束，运营者值才能"落进"类型分支完成覆盖；裸 `*60`（无类型分支）干脆是语法错误。**建议**：底座把"发行配方中可覆盖字段必须带 default 标记"定为 lint 约定/评审项，这是整套 L3/L4 语义的命门。附带收益（实证）：L4 只能改值、不能改约束，越界覆盖（maxAttempts=99）被拒——这正是设计纪律里"运营者空间受限"想要的治理语义。

4. **引用完整性可静态校验（超出预期的强项）**：`trigger: or([for k, _ in evalFunctions {k}])` 把 map 键集转成析取，安灯→评估函数、combo→规则 id、required→工件字段三类悬空引用全部在 vet 期拒绝。**坑**：这种 `or()` 析取必须内联进约束里，不能落成独立的 hidden 字段——未解析析取会被 vet 判为 incomplete 而失败（探针 P7）。另外 `struct.MinFields()` 这类标量校验器必须"落字段 + 和 true unify"才构成断言，直接写在 struct 体里会与 struct 冲突。

5. **表达力边界（十条规则实测）**：
   - **舒适区**：阈值、比例（含 `critical > warn` 跨字段约束）、时间窗/指纹参数、升级链参数（`afterMs` 严格递增用"检查式 comprehension 吐 bool + 与 `[...true]` unify"静态查出，实证 `_check.0: conflicting values false and true`）、去重（`list.UniqueItems` 返回 bool，`_u: list.UniqueItems(xs)` + `_u: true` 断言即可）。
   - **别扭**：列表内部关系类检查都要绕 comprehension 惯用法；kind 析取失败时报 `8 errors in empty disjunction` 根因被淹没（DX 差，底座需要包一层友好报错）；无 lambda、无 fold。
   - **做不到（也不该做）**：时间相关/事件流谓词——"now 是否在静默窗内（跨零点）""连续 3 个采样越限""10 分钟内 ≥3 次失败"在 CUE 里只是参数对象，无法求值。
   - **受限 DSL 要求：成立**。CUE 无副作用、无 IO、无用户递归（结构环报错）、求值必终止且可静态校验——"类型化受限 DSL"是它的构造性质，不需要额外发明沙箱。但要认清它是**规则参数与参数间约束的 schema 语言，不是规则引擎**。

6. **运行时求值的代价与路线**：官方实现只有 Go 库（cuelang.org/go）。Rust 生态（本次 crates.io 实查）：`cue`/`cue-rs` crate 是乐谱库 libcue 的绑定（与 cuelang 无关）；`cuengine`（cuenv/cuenv，Go-Rust FFI bridge，21k 下载，2026-04 更新）存在但**本次未测**。建议：
   - **主路线（推荐）**：部署期预编译——改配方时 `cue vet` + `cue export` 落 JSON，运行时读 JSON，零 CUE 依赖、零进程开销；
   - **进程路线**：spawn `bin/cue export`，实测 ~7-17 ms/次，低频（每次配方加载/合并）完全可接受，事件级高频不可行；
   - **运行时谓词**：数据相关条件用 CEL（`cel-interpreter` crate 0.10.0，1M+ 下载）在 Rust 内嵌求值；CUE 只负责表达式串的"存在性 + 参数闭合"静态校验。rhai 这类图灵完备脚本不合规（设计纪律明令排除）。

7. **总回答**：CUE 同时当产线定义与配方 DSL **够用且是最优选**，缺的不是表达能力而是三件配套：①运行时求值路径要预编译或接受进程开销（7 ms 级）；②数据相关谓词需另配受限求值器（CEL）并由 CUE 静态看住参数；③析取类 schema 的报错 DX 需要底座封装。

## ⑤ 遗留问题

- `cuengine`（Go-Rust FFI bridge）未实测——本 PoC 只验证了 CLI 进程路线；若不想走预编译可补测。
- 未验证 Go 侧 `cuelang.org/go` 库嵌入（本机无 Go 工具链）。
- CEL 表达式串的静态校验环节（parse + 参数闭合检查）未做 PoC，只定了方向。
- 大配方（千条规则）下的 vet/export 性能未测；当前 10 条 17 ms。
- `cue vet` 对 incomplete 的默认拒绝行为（v0.17：不加 `-c` 也会因 incomplete 失败）在 CI 里的标准姿势（何时 `-c`/`-c=false`）未系统测试。
