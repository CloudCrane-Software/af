# VERIFY-LOAD.md — CUE→JSON→内核装载端到端 + 千条规则性能（poc-cue 遗留补测）

对应 `poc-cue/RESULT.md` ⑤遗留："大配方（千条规则）下的 vet/export 性能未测""（装载错误分类学）"。本次全部真实执行，2026-10-19，本机 16 核/14G，cue v0.17.1（`af/bin/cue`），rustc/cargo 1.99.1（构建一律 `-j4`）。

## ① 结论

**通过（含一条对底座方案的红色告警）**。装载端到端成立：CUE schema → `cue export` → JSON → serde 装载器 → 类型化结构 + 语义校验，正例通过、17 个坏样例全部被拒且错误码逐一命中（13/13 测试通过：12 集成 + 1 单测）；千条 bundle 装载仅 **5.8 ms p50**（release）。**红色告警在 CUE 侧：千条规则的配方在 poc-cue 现行规则结构（combo 用 `or()` 做引用完整性）下，`cue vet` 88.9 s / `cue export` 64.9 s（p50），且增长近平方（100→500 条耗时 ×34.6）**。对照实验（去掉 combo/`or()`，其余不变）回到近似线性：1000 条 vet 1.06 s / export 1.36 s，差 **47–84 倍**。决策输入：**千条级配方的引用完整性检查必须移出 CUE（由装载器 E2001 承担，本次已实现并验证），CUE 侧只做 per-rule closed schema 校验；或按 kind/批分片 vet**。错误分类学落成三层 15 码（E0 版本/E1 形状/E2 语义 + E9 部署期 CUE 阶段），其中"可覆盖字段缺 default 标记"经实验证明**在 JSON 里不可见**（裸具体值 30 与 `*30 | int & >0` 的 export 字节级相同），只能部署期在 CUE 源上查——这是分类学分层的关键依据。版本策略建议**拒绝式**（信封层非负整数大版本，payload 解析前门禁，未知版本一律 E0001，不做兼容尝试）。

## ② 步骤（做了什么）

```
verify-load/                          # 本次任务目录（全部新增文件在此）
├── gen_rules.py                      # 千条规则配方生成器（poc-cue rules 结构上扩展，8 kind 轮转）
├── perf_timing.py                    # 主实验：cue vet/export @ 10/100/500/1000，5 runs/cell
├── perf_control.py                   # 对照实验：去 combo/去 or()，3 runs/cell
├── perf/rules-{10,100,500,1000}.cue  # 生成的配方实例（含 or() 版）
├── perf/rules-nocombo-{...}.cue      # 对照组实例
├── build_bundle.py                   # 端到端拼包：cue export(line)+cue export(rules) → {schemaVersion,line,rules}
├── make_bad_samples.py               # 从 good-bundle.json 逐类变异出 17 个坏样例
├── samples/good-bundle.json          # 正例（真实 cue export 产物 + 信封）
├── samples/bad-e*.json               # 17 个坏样例（文件名即预期错误码）
├── samples/cue/marker-*.cue          # E9001 default-marker 类的 CUE 侧坏样例 + 正例 + L4 覆盖
└── loader/                           # 独立 cargo 工程（自有空 [workspace]，未挂 AF workspace）
    ├── Cargo.toml                    # 依赖仅 serde(derive) + serde_json
    ├── src/error.rs                  # 错误分类学：LoadError 枚举（15 码）+ code()/kind_name()/Display
    ├── src/model.rs                  # 类型化结构：#Line/#Rule 全量映射（deny_unknown_fields，serde default 映射 *默认）
    ├── src/loader.rs                 # 三段管线：版本门禁 → serde 结构解析 → 语义校验（引用/值域/唯一/非空）
    ├── src/main.rs                   # CLI：装载（exit 0/2）+ --bench
    └── tests/taxonomy.rs             # 17 坏样例逐一断言错误码 + 版本策略 5 测 + cue 真实触发归类 E9001/E9002
```

流程：生成配方 → 计时主实验/对照实验 → 真实 cue export 拼 bundle → 装载器三级管线 → 每类造坏样例触发断言 → 千条装载计时。

## ③ 证据（命令原文与输出摘录）

### ③-1 千条规则性能（主实验，5 runs/cell，中位）

```
$ python3 perf_timing.py
cue v0.17.1, 5 runs/cell, schema=.../poc-cue/rules/rules.cue
N	fileKB	vet_min	vet_p50	vet_mean	exp_min	exp_p50	exp_mean
10	1.5	44.1	75.0	67.7	33.4	37.7	37.8
100	12.7	405.4	472.7	477.8	411.3	472.1	464.1
500	63.3	11136.6	16345.1	16184.9	8231.4	9864.0	10295.4
1000	126.6	53501.1	88929.2	116232.3	50045.5	64887.9	59464.0

增长（相对前一档的倍数）: N → vet ×, export ×
10→100: N×10 → vet ×6.3, export ×12.5
100→500: N×5 → vet ×34.6, export ×20.9      # 近平方，超线性实锤
500→1000: N×2 → vet ×5.4, export ×6.6
```

锚点（与 RESULT.md 任务4 可比）：原 PoC 样例 `cue vet rules/rules.cue rules/rules-10.cue` ×5 → **p50 26.9 ms**（RESULT.md 曾测 17 ms，同量级，机器状态差异）；export p50 40.0 ms。

### ③-2 对照实验：去掉 combo/`or()`（其余完全不变，3 runs/cell）

```
$ python3 perf_control.py
对照组（无 combo / 无 or()），3 runs/cell
N	vet_min	vet_p50	exp_min	exp_p50
10	40.0	51.3	47.0	55.8
100	192.5	210.0	162.2	190.9
500	536.9	599.0	548.0	559.4
1000	1053.5	1061.0	1358.0	1364.7

10→100: N×10 → vet ×4.1, export ×3.4
100→500: N×5 → vet ×2.9, export ×2.9        # ≈线性
500→1000: N×2 → vet ×1.8, export ×2.4
```

**N=1000 时主实验 vs 对照：vet 88.9 s vs 1.06 s（×84），export 64.9 s vs 1.37 s（×48）。** 根因：`rules-10.cue` 里的 `_ruleIds: or([for r in _ruleDefs {r.id}])`（RESULT.md ③任务3 的引用完整性手法）让每条 combo 规则都统一一个 N 宽析取，规模平方级放大。

### ③-3 装载器（cargo test，13/13 通过 = 12 集成 + 1 单测）

```
$ cd loader && cargo test -j4
test codes_are_unique ... ok                  # 错误码互不重复（lib 单测）
test taxonomy_every_bad_sample_hits_its_code ... ok   # 17 个坏样例逐一命中预期码
test good_bundle_loads ... ok                 # 正例 + serde default（*true/*0/?缺省）映射
test version_gate_runs_before_payload_parse ... ok
test version_zero_and_future_are_rejected ... ok
test version_must_be_non_negative_integer ... ok
test missing_version_is_rejected_even_with_valid_payload ... ok
test only_the_three_envelope_keys_are_allowed ... ok
test dangling_combo_error_names_the_target ... ok
test malformed_json_is_type_mismatch_at_root ... ok
test cue_stage_default_marker_missing_is_e9001 ... ok  # 测试内真实运行 cue 二进制
test cue_stage_ok_marker_exports_and_overrides_cleanly ... ok
test cue_vet_rejection_maps_to_e9002 ... ok
test result: ok. 12 passed; 0 failed
```

### ③-4 端到端 + 17 个坏样例的 CLI 触发（原文摘录）

```
$ loader/target/debug/line-loader samples/good-bundle.json
{"ok":true,"schemaVersion":1,"line":"line-a","rules":10,"andonRules":2,"tools":3,"evalFunctions":3}   exit=0

$ for f in samples/bad-*.json; do line-loader $f; echo exit=$?; done   # 全部 exit=2，错误码全对
bad-e0001-version-unsupported  → E0001 SCHEMA_VERSION_UNSUPPORTED  "schemaVersion 2 不支持，当前支持 1（拒绝装载，不做兼容尝试）"
bad-e0002-version-missing      → E0002 SCHEMA_VERSION_MALFORMED    "字段缺失"
bad-e0002-version-wrongtype    → E0002 SCHEMA_VERSION_MALFORMED    "必须是非负整数，得到 '1'"
bad-e0003-envelope-unknown     → E0003 ENVELOPE_UNKNOWN_FIELD      "信封层未知字段 `owner`"
bad-e1001-unknown-field        → E1001 UNKNOWN_FIELD               "未知字段 `timeoutSec`（/line）"   ← 对应 line-bad-nested.cue 同型
bad-e1002-type-mismatch        → E1002 TYPE_MISMATCH               "invalid type: string '2000', expected u64（/line）"
bad-e1003-missing-field        → E1003 MISSING_FIELD               "缺少必填字段 `notify`（/line）"
bad-e1004-unknown-variant      → E1004 UNKNOWN_VARIANT             "`canbus` 不在 driver 取值表"
bad-e1007-kind-params-mismatch → E1007 KIND_PARAMS_MISMATCH        "rule-0 kind=threshold 的 params 含其它 kind 的字段 `critical`"
bad-e1008-unknown-kind         → E1008 UNKNOWN_KIND                "规则 `rule-1` 未知 kind `quantum`" ← 对应 rule-bad-5.cue 同型
bad-e2001-dangling-combo       → E2001 DANGLING_REFERENCE          "/rules/7/params：rule `rule-ghost` 未声明"
bad-e2001-dangling-trigger     → E2001 DANGLING_REFERENCE          "evalFunction `noSuchFn` 未声明"   ← 对应 line-bad-ref.cue 同型
bad-e2001-dangling-required    → E2001 DANGLING_REFERENCE          "artifactField `ghostField` 未声明"
bad-e2002-critical-below-warn  → E2002 VALUE_OUT_OF_RANGE          "critical(0.001) 必须严于 warn(0.015)" ← 对应 rule-bad-3.cue 同型
bad-e2002-chain-flat           → E2002 VALUE_OUT_OF_RANGE          "升级链 afterMs 必须严格递增：300000 <= 300000" ← 对应 rule-bad-4.cue 同型
bad-e2003-duplicate-id         → E2003 DUPLICATE_ID                "rule id 重复：`rule-0`"
bad-e2004-empty-notify         → E2004 EMPTY_COLLECTION            "集合 `notify` 不应为空（/line/andonRules/0）"
```

### ③-5 E9001：default-marker 类只能在 CUE 侧查出（JSON 不可见性实验）

```
$ cd samples/cue
$ ../../../bin/cue export marker-ok.cue                 # 正确形态 *30 | int & >0
{"batchIntervalSec": 30}                                exit=0
$ ../../../bin/cue export marker-missing.cue            # E9001-a 连默认都没写
batchIntervalSec: incomplete value >0 & int             exit=1
$ ../../../bin/cue export marker-lost.cue               # E9001-b 忘了 *（30 | int 裸值被吸收，marker 丢失）
batchIntervalSec: incomplete value 30 | >0 & int        exit=1
$ ../../../bin/cue export marker-bare.cue marker-override.cue   # E9001-c 裸具体值 + L4 覆盖
batchIntervalSec: conflicting values 15 and 30          exit=1   # 冲突而非覆盖
$ ../../../bin/cue export marker-ok.cue marker-override.cue     # 正例
{"batchIntervalSec": 15}                                exit=0
# JSON 不可见性：裸具体值与带 marker 的字段 export 出来字节级相同 →
$ ../../../bin/cue export marker-bare.cue | tr -d ' \n'   ==   {"batchIntervalSec":30}
$ ../../../bin/cue export marker-ok.cue   | tr -d ' \n'   ==   {"batchIntervalSec":30}
```

### ③-6 千条 bundle 端到端装载耗时（Rust 侧）

```
$ python3 build_bundle.py 1000 perf/bundle-1000.json     # 真实 cue export 流水（此处 export 耗时即 ③-1 的 64.9 s）
wrote perf/bundle-1000.json: schemaVersion=1 rules=1000 bytes=213575
$ loader/target/release/line-loader --bench perf/bundle-1000.json 200
n=200 bytes=213575 min=3.008ms p50=5.818ms mean=6.573ms
$ --bench perf/bundle-100.json 200   → p50=0.512ms
$ --bench samples/good-bundle.json 200 → p50=0.087ms      # 10 条
（debug 版千条：p50=18.9ms）
```

## ④ 对最终方案的反馈/决策输入

1. **【红色告警】千条级配方不能用现行 rules 结构整体 vet/export**。combo 的 `or()` 引用完整性析取使成本近平方（③-1），1000 条达分钟级；去掉 `or()` 即回线性（③-2）。底座三选一：**a)（推荐）引用完整性移到装载器语义层**（本次 E2001 已实现同强度检查：combo.allOf/andon.trigger/artifact.required 三类悬空引用全拒）——CUE 侧保住 per-rule closed schema 的快速校验；b) 分片 vet/export（按 kind 或每 ≤100 条一片，代价是跨片引用要装载器兜底）；c) 接受分钟级编译并把它彻底锁死在部署期缓存里（配方变更频率低时可忍，但 CI 会疼）。
2. **装载器与 CUE 的职责切分（实测依据）**：形状类错误（E1xxx）CUE closed schema 与 serde `deny_unknown_fields` 等价双保险；语义类（E2xxx）两处都做（CUE `or()`/跨字段约束 + 装载器复检）——装载器是最终防线，因为内核可能面对未经 cue vet 的 JSON。**default 标记类（E9001）是唯一 CUE 独占**：JSON 里不可见（③-5 字节级相同实验），必须部署期 lint，评审清单里"可覆盖字段必须 `*值 | 类型约束`"这条约定要靠它落地。
3. **错误分类学三层 15 码**（`loader/src/error.rs`，单测保证无重码）：E0xxx 信封/版本（E0001 版本不支持/E0002 版本残缺/E0003 信封未知字段）、E1xxx 形状（E1001 未知字段/E1002 类型不符/E1003 缺字段/E1004 枚举值不在表/E1007 kind-params 错配/E1008 未知 kind）、E2xxx 语义（E2001 悬空引用/E2002 值域/E2003 id 重复/E2004 非空违约）、E9xxx 部署期（E9001 default-marker 缺失/E9002 cue vet 拒绝，管线包装层用同一枚举上报，本次在 cargo test 里真实运行 cue 验证了触发条件与归类）。已知边界：serde_json `from_value` 的错误不带 JSON 路径，path 由管线按解析层拼接（规则内 params 级别可定位，字段级定位需 path-tracking 反序列化器，见 ⑤）；E1004 的"字段名"是占位文本，取值本身是准的。
4. **版本策略（④任务）：拒绝式，不做兼容尝试**。`schemaVersion` 放**内核装载信封**（`{schemaVersion, line, rules}` 顶层），不放各领域 CUE schema 内——CUE 文件保持独立可 vet/可 export，版本是内核装载体的事。取值**非负整数大版本**（当前支持 1）。门禁在 payload 解析之前（测试 `version_gate_runs_before_payload_parse` 证明版本错优先于一切形状错）。拒绝而非兼容尝试的理由：① closed-schema 纪律（RESULT ④-1）下旧装载器对新字段本来就必然 E1001，"兼容尝试"只是把一个清晰的 E0001 换成一串难懂的 E1001；② 装载发生在内核启动、配置与内核同源部署，版本偏差是部署事故，应 fail-fast 精确报错；③ 未来若真要双版本，在信封层按整数分发到版本化枚举即可，本次骨架已把门禁独立成段，扩展点现成。
5. **装载成本可忽略，编译成本才是墙**：千条装载 5.8 ms（release）vs CUE export 1.36 s（无 or() 也要），再佐证 RESULT.md 的预编译主路线——JSON 工件运行时装载毫无压力；要把千条配方编译出 CI，先解决 ④-1。
6. serde 两个坑给后续写装载器的人：**`flatten` 与 `deny_unknown_fields` 不兼容**（internally-tagged enum 方案因此废弃，kind/params 分派手写，反而换来 E1007/E1008 精确码）；serde_json 的 missing field 归 `Data` 类不是独立 Category，报文 sniff（`` missing field `x` ``）要跳过开头反引号取第二个（本次测试抓出过空字段名 bug）。

## ⑤ 遗留问题

- 错误定位粒度：结构错误目前定位到"解析层/规则下标/params"，字段级 JSON path 需要 path-tracking 反序列化器（如 serde_path_to_error）或手写 Visitor，未做。
- 装载器未覆盖的 CUE 侧约束（部署期负责、装载器未复检）：字符串正则类（id 形态 `^[a-z][a-z0-9-]*$`、时长 `\d+(ms|s|m|h)$`、时间窗 `\d{2}:\d{2}`、图模板 semver）与数值上下界（timeoutMs ≤10000 等）；如需装载器兜底可机械补上，本次按"防结构错误+防悬空引用+防跨字段违约"划界。
- E9001 目前是"触发条件验证 + 枚举归类"，还没有做成对配方 CUE 源的批量 lint 工具（扫所有可覆盖字段是否带 `*` 标记）。
- 千条级下装载器语义校验 O(N)（HashSet 查引用），未测十万条以上；无 or() 的 CUE 侧校验在 100→1000 呈轻微超线性（×10 条 ×5-7 耗时），更大规模若成为需求需再测。
- FFI 路线（cuengine）在千条配方下的求值性能仍未测（VERIFY-FFI.md ⑤ 同款遗留）；鉴于 ③-1/③-2 结论，若走 FFI 也必须先解决 or() 放大问题。
- `loader/target/`（226 MB，debug+release）已清理；复测命令：`cd verify-load && python3 perf_timing.py && python3 perf_control.py && python3 build_bundle.py 10 samples/good-bundle.json && python3 make_bad_samples.py && cd loader && cargo test -j4`。
- 卫生声明：未改 AF 根 Cargo.toml（复核仍为 `members = ["crates/*"]`）；loader 自带空 `[workspace]` 独立于 AF workspace；本次未启动任何常驻进程，无端口占用。
