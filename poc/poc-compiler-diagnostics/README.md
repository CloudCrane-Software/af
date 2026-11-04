# PoC · 编译器诊断的机读产物面（G2）

分支 `feature/compiler-diagnostics-machine`（基线 `product/main` = 6ad146f）。

## 改了什么、为什么

**缺口（G2）**：编译器诊断只出现在人类可读的文本/错误链里——CI、编辑器、
外部对账器拿不到结构化判定（退出码/文本不可单独作判据）。

**落地面（三件，全部在既有 crate 内，零新增 crate/机制/码）**：

1. **`crates/af-compiler/src/diagnostics.rs`（新模块，~230 行）**——schema
   `af.diagnostics.v1` 的 serde 结构面（`DiagnosticsFile`/`Diagnostic`/
   `DiagLocation`/`Phase`/`Severity`）+ 三个纯投影函数：
   - `from_load_error(&af_dsl::LoadError)` → phase=load（E0001-E0004/E1xxx）
   - `from_validate_report(&af_dsl::Report)` → phase=validate（E2xxx 族）
   - `from_bind_violations(&[BindViolation])` → phase=bind（E9001-E9003）
   - `stamp_source_file` 在 CLI 面给条目盖源文档路径（行/列 null，不编造）。
   码逐字来自既有 `ErrorCode::code()` / `BindCode::as_str()`；message 逐字
   来自既有 `Violation::message` / `BindViolation::message`；**零判定零改写
   零新码**——产生条件、拒绝语义、码值不动，本面只是多一个出口。
2. **`crates/af-compiler/src/bin/af-compiler-plan.rs`**——加
   `--diagnostics-out <产物路径>` 旗标：源文档读取成功后的**每条**退出路径
   （成功与失败）把产物落盘；不给旗标时行为与扩展前逐字节一致（承
   `--registry` P2-L10 同款纪律；CLI 测试 `cli_without_flag_writes_no_product_
   and_keeps_exit_code` 断言产物不落盘 + 退出码不变）。
3. **`crates/af-compiler/src/lib.rs`**——模块注册 + 再导出。

**测试（`tests/diagnostics_test.rs` 15 条 + `tests/diagnostics_cli_test.rs`
5 条，先红后绿）**：
- 逐码族产物用例：E0001/E1002（load）、E2001（validate）、E9001/E9002/
  E9003（bind）——核 code/severity/phase/node/reference/rule 逐字；
- 负例：零违例校验/绑定 → 空条目；成功路径产物 = 空数组 + schema 在场；
- schema 稳定性：golden 字面量（逐字节冻结）、键集断言、枚举值字面量、
  反序列化回环、确定性（同输入两次投影逐位一致）；
- CLI 面真跑 bin：失败/成功/无旗标/旗标误用四面。

## 真跑证据（本目录）

`run-evidence.log`（命令 + exit + stderr 全录）；输入在 `inputs/`：

| 路径 | 输入 | exit | 产物 | 产物内码集 |
|---|---|---|---|---|
| 失败 · load 相位 | `inputs/failure-e0001.yaml`（缺 version） | 1 | `failure-e0001.diagnostics.json` | `[("E0001","load")]` |
| 失败 · validate 相位 | `inputs/failure-e2001.yaml`（边指向 ghost 节点） | 1 | `failure-e2001.diagnostics.json` | `[("E2001","validate")]` |
| 失败 · bind 相位 | `inputs/failure-e900x.yaml`（空注册表 fail-closed） | 1 | `failure-e900x.diagnostics.json` | `[("E9001","bind")×2]`（stdout 0 字节：不输出残缺计划） |
| 成功 | `crates/af-compiler/testdata/src/sample2.yaml` × `--registry af/packages` | 0 | `success-sample2.diagnostics.json` | `[]`（stdout 计划 JSON 照常，见 `success-sample2.plan.json`） |

产物片段（`failure-e2001.diagnostics.json`）：

```json
{
  "schema": "af.diagnostics.v1",
  "diagnostics": [
    {
      "code": "E2001",
      "severity": "error",
      "message": "边引用未声明节点 to=\"ghost\"",
      "phase": "validate",
      "location": { "file": "poc/poc-compiler-diagnostics/inputs/failure-e2001.yaml", "line": null, "column": null },
      "node": "root",
      "reference": null,
      "rule": "dangling_reference"
    }
  ]
}
```

字段正本：`SCHEMA.md`（每字段类型/语义/取值来源 + 空数组语义）。

## 做不到什么（边界，如实登记）

1. **无码失败面不入产物**：组件清单装载失败（`ComponentRegistryError`）、
   图清单装载失败（`af_dsl::registry::RegistryError`）、lowering 内部拒绝
   （`LowerError`/`PlanError::Lower`——不变量击穿，正常管线不可达）、序列化
   不可达错误——这些面**没有既有稳定码**，本批按「不许另编一套码」纪律
   不给它们造假码，产物落空数组 + stderr 点名 + 非零退出码。**是否为它们
   登记码位 = 需要人类裁决**（登记须走码表文档：码/含义/触发条件/为什么
   旧码不够）。消费方契约：**空数组 ≠ 成功**，成败以退出码为准。
2. **行/列恒 null**：af-dsl 校验器不记 span（违例只有 scope 字符串），产物
   不编造位置。补行/列需校验器面记 span（跨 crate 改动），超出本批
   「只多一个出口」的边界。
3. **只接了 `af-compiler-plan` 一个出口**：`af-compiler-snapshot`（stage 1-2
   子集）与 `af-compiler-construct`（建设组包面）未接同款旗标——可平移，
   本批不做（任务书只要一个出口）。
4. **severity 恒 "error"**：既有码族全部 fail-loud，无 warning 级诊断存在；
   字段位占住（将来加值不改名）。
5. **兄弟缺口登记（本批不做，仅登记）**：G3「manifest 缺席 = 不核对的
   未组包破窗」（`package.rs:136-139`）、G4「K5 保护面仅手工登记四件」、
   G5「clippy 增量基线门强度」、G6「compiler_version 仅溯源不核对」。

## 门（读数见交付报告）

`cargo test --workspace`（0 failed）、`cargo clippy --workspace --all-targets
-- -D warnings`、`python3 -I ci/clippy_gate.py <jsonl>`。
