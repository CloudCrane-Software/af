# PoC G3：manifest 缺席 = 不核对的未组包破窗——收口留证

分支 `feature/g3-manifest-absent`（基线 = 开工时 origin/product/main）。
v0.3「编译器内建反馈现状」盘点缺口 G3：`verify_package_manifest` 在 manifest
缺席时 `return Ok(None)`，在要求核对的路径上与「核对过且通过」不可区分——
**没组包（或删掉 manifest）的任务包静默拿到"成功但什么都没核对"**。

## 改了什么

1. **类型分开（af-compiler/src/package.rs）**：新增
   `pub enum ManifestVerification { Verified(TaskPackageManifest), Absent }`；
   `verify_package_manifest` 返回 `Result<ManifestVerification, PackageError>`。
   缺席 = 显式 `Absent`（未核对状态，可机检，与 `Verified` 值域不相交）；
   在场核对判据**逐位原样**（task/plan sha256 与留档比对，任一不符即
   `ContentTampered`，文案含「清态换号重提」）。lib.rs 导出面加
   `ManifestVerification`。
2. **要求核对的路径 fail-closed（af-order/src/cli.rs）**：
   `preflight_package_manifest`（生产唯一接线，正式 submit + 干跑同点）对
   `Absent` 返回 `OR1001` 拒绝，文案点名缺席文件路径 + 「删 manifest 曾可
   绕过内容核对」+ 行动指令（经 af-compiler 组包重产，勿手工拼包）。拒于
   一切副作用之前。
3. **允许路径显式表达（af-order/src/cli.rs）**：干跑留证面 `package_manifest`
   字段在核对不适用时（非 graph/report 档 / 无 RealExec）从含混 `null` 改为
   显式 `"not_applicable"`；核对过 = manifest 留档对象，两态机检可辨。

## 为什么

- 改前实证（`before-stdout.txt` E2）：**无 manifest 的 graph 任务包在正式
  submit 路完整走通**——签凭证、起引擎、17 turns / 107310 tokens、MES 4 事件、
  账本 8 笔。攻击面不是理论：改 task.json/plan.json 后删掉 manifest 即绕过
  全部内容核对（E2G2 实证预算帽全在 plan.json）。
- `Ok(None)` 的"成功"形态让调用方**无从拒绝**：不 match 就误读为通过。类型
  上分开 + 生产路径 fail-closed，让"没组包"这件事不能静默地看起来像
  "核对过了"。

## 改前改后对照

见 `comparison.md`（同探针同场景：E1 缺席干跑 放行→拒；E2 缺席正式 整单真跑→
preflight 即拒零残留；E3 被改 / E4 一致 两态逐字不变——判据未动）。
原始 stdout：`before-stdout.txt` / `after-stdout.txt`；探针副本 `g3_probe.rs`
（跑时临时挂 af-order/tests/，跑完即撤，不入库）。

## 调用点清单

见 `call-sites.md`（直接调用 6 点 + 受影响面 8 点，逐点标明变/不变与理由）。

## 测试

任务书四场景全覆盖：
①缺席→要求核对路径拒：`manifest_absent_is_rejected_fail_closed_before_any_side_effect`
（干跑 + 正式两路）；②缺席→允许路径显式未核对：
`verify_marks_absent_manifest_as_explicit_absent_state`（af-compiler 层
`Absent` 状态）+ `non_verifying_unit_dryrun_reports_manifest_check_not_applicable`
（af-order 层 `not_applicable`）；③在场被改→`ContentTampered`：既有
`verify_rejects_tampered_task_or_plan…` / `submit_preflight_rejects_tampered…` /
`manifest_tampered_itself_is_rejected` 断言一字未动；④在场一致→过：既有
roundtrip / `dryrun_passes_with_manifest…` 断言一字未动。

## 门读数（全绿）

- `cargo test --workspace`：exit 0，**1511 passed / 0 failed**（152 个测试段全 ok）
- `cargo clippy --workspace --all-targets -- -D warnings`：exit 0
- `cargo clippy --workspace --all-targets --message-format=json` → error 级 0 条 →
  `python3 -I ci/clippy_gate.py`：**PASS**（0 警告 / 0 签名，基线 0 签名未动）

## 如实登记的边界（做不到/未做/翻转了什么）

1. **两条既有断言被翻转**（非削弱，是任务书指定的行为翻转）：
   - `manifest_absent_passes_through` → `manifest_absent_is_rejected_fail_closed…`
     （旧断言把"缺席放行"制度化，正是破窗正本）；
   - `verify_passes_through_when_manifest_absent` → `verify_marks_absent_manifest_
     as_explicit_absent_state`（Ok(None) → Ok(Absent)，语义保持"不报错 + 非核对
     过"并显式化）。两条的场景与"非通过"期待延续，断言从含混变精确。
2. **两处测试夹具补 manifest**（断言一字未动）：`compile_to_submit_one_hop…` 与
   `dryrun_rejects_broken_plan…` 的夹具原本不带 manifest（遗漏而非本意），
   fail-closed 后会死在 preflight 测不到本意面；补的是"对夹具现状现算留档"，
   测试继续测各自本意（干跑链 / 装订预检），改后实测 PASS。
3. **不适用路径的语义选择**：swev/stub 档与无 RealExec 路径**维持不核对**（本
   批不改判据适用范围，只改缺席语义的表达与处置）；干跑留证已从 null 显式化为
   not_applicable，正式路径这些档的 submit 输出面没有 manifest 字段可标（输出
   面本就无此字段），未额外造新输出面。
4. **历史旧包**：manifest 面（E1 缺口②）上线前手工产的旧任务包，graph/report
   档再提交即被拒——这是有意的（fail-closed），出路是经 af-compiler 重组包
   （组包确定性，重产即留档），文案已给出行动指令。
5. 探针 E2 改前跑真引擎耗了 17 turns / 约 0.17 美分（测试订单 E2G-G3-1，本地
   state，已随 tmp 清理）——这是改前证据的取证成本，如实登记。
