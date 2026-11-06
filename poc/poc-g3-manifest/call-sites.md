# G3 逐调用点清单（verify_package_manifest 及受影响面）

复核方式：`grep -rn "verify_package_manifest" --include="*.rs" crates/`（施工时全仓
扫描），生产接线唯一性成立——除 af-compiler 自身导出/定义外，生产代码调用仅
af-order/src/cli.rs 一处；其余全部是测试。

## 直接调用 verify_package_manifest 的点

| # | 位置 | 性质 | 行为变化 | 说明 |
|---|---|---|---|---|
| 1 | `af-order/src/cli.rs` `preflight_package_manifest`（run_submit:335 调用，正式 + 干跑同点） | **生产唯一接线** | **变**：graph/report 档 + RealExec 下缺席从「Ok(None) 静默放行」翻转为 **OR1001 fail-closed 拒**；在场核对判据逐位不变；非 graph/report 档或无 RealExec 仍不适用（None），但干跑留证从 `null` 改为显式 `"not_applicable"` | 破窗关闭点。match 三分支：Verified→Ok(Some)；Absent→Err；verify 的 Err→透传 OR1001 |
| 2 | `af-compiler/tests/package_manifest_test.rs` roundtrip | 测试 | 形态适配：`.unwrap().expect(..)` → let-else 解构 `Verified` | 场景与期待不变（在场且一致 → 核对过且取回留档），断言语义保留 |
| 3 | 同上 `verify_rejects_tampered_task_or_plan…`（两处 unwrap_err） | 测试 | **不变** | Err 路径类型未变（ContentTampered 判据原样） |
| 4 | 同上 `verify_marks_absent_manifest_as_explicit_absent_state`（原 `verify_passes_through_when_manifest_absent`） | 测试 | **语义翻转适配**：断言从 `Ok(None)`（缺席=不核对）改为 `Ok(ManifestVerification::Absent)`（缺席=显式未核对状态） | 旧断言编码的正是本批要关的破窗口径；新断言保持「不报错 + 非『核对过』」语义并加强为可机检枚举。**如实登记：这是断言期待的翻转，依据任务书测试要求②** |
| 5 | 同上 `verify_rejects_malformed_manifest`（两处 unwrap_err） | 测试 | **不变** | 坏形判据原样 |
| 6 | `af-compiler/tests/construction_teaching_test.rs:112` | 测试 | 形态适配：`.is_some()` → `matches!(Verified(_))` | 场景与期待不变（组包后现场核对过） |

## 不直接调用 verify、但行为受 preflight 翻转影响的面

| # | 位置 | 行为变化 | 处置 |
|---|---|---|---|
| 7 | `af-order/tests/package_manifest_preflight_test.rs` `manifest_absent_is_rejected_fail_closed…`（原 `manifest_absent_passes_through`） | **翻转**：旧断言（缺席干跑放行 + checks 不点名 + evidence null）编码的正是破窗；新断言：干跑 + 正式两路 OR1001 拒、点名缺席文件、零残留。**如实登记：断言期待翻转，依据任务书测试要求①；改前实证见 comparison.md E2** |
| 8 | 同文件 `dryrun_passes_with_manifest…` / `submit_preflight_rejects_tampered…` / `manifest_tampered_itself_is_rejected` | **不变**（在场三场景：过/内容被改拒/manifest 自身被改拒，断言一字未动） | — |
| 9 | 同文件 新增 `non_verifying_unit_dryrun_reports_manifest_check_not_applicable` | **新增**：不适用档干跑留证 `not_applicable`（任务书测试要求②的 af-order 面） | — |
| 10 | `af-order/tests/compile_submit_dryrun_test.rs` `compile_to_submit_one_hop…` | 夹具**补** manifest（对落盘编译产物现算留档）；断言一字未动 | 该测试本意测干跑链（摘要钉死/装订/档案预检），夹具原本无 manifest 属遗漏——fail-closed 后会死在 preflight，测不到本意面。补留档后继续测原面 |
| 11 | 同文件 `dryrun_rejects_broken_plan…` | 夹具**补** manifest（对**破损后**的 plan.json 现算留档）；断言一字未动 | 本意测装订预检对非编译产物形 fail-loud；不补则死在 preflight 缺席拒，测不到装订面。留档针对现状文件，manifest 核对过，失败点仍落装订预检（改后实测 PASS，文案「干跑计划装订预检未过」） |
| 12 | 同文件 `dryrun_flag_parse_is_strict` / `compiler_assembled_teaching_package…` | **不变**（前者错误在参数解析期，早于 preflight；后者组包自带 manifest） | — |
| 13 | `af-order/src/cli.rs` 内部测试（tests/w1_tests：stub 档 submit、parse、目录解析） | **不变**（全部非 graph/report 档，不进 manifest preflight） | — |
| 14 | 其余 crate（af-execline/af-engine/…测试） | **不变**（不触 manifest 链；workspace 全量 1511 passed / 0 failed 佐证） | — |

## 类型面变化（调用方无法误读的机制）

```rust
// af-compiler/src/package.rs（新）
pub enum ManifestVerification {
    Verified(TaskPackageManifest), // 核对过且通过（携留档）
    Absent,                        // 缺席 = 未核对（不是通过；调用方必须显式处理）
}
pub fn verify_package_manifest(task_dir: &Path) -> Result<ManifestVerification, PackageError>;
```

- 旧 `Ok(None)`：缺席与「不适用」「核对过」全靠调用方自觉区分——不 match 就误读。
- 新形态：拿到 `ManifestVerification` 必须穿过枚举才能取值；`Absent` 与 `Verified`
  值域不相交（`PartialEq` 可机检）。af-order preflight 对 `Absent` 拒绝，不适用路径
  （非 graph/report 档 / 无 RealExec）返回的 `Option::None` 只剩「不适用」一义。
