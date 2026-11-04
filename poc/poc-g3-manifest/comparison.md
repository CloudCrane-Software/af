# G3 改前 vs 改后行为对照（同一探针、同一场景、同一环境）

探针：`g3_probe.rs`（本目录留档副本；跑时临时挂 `crates/af-order/tests/`，跑完即撤，
不入库）。场景全部为 **graph 档 + RealExec 三件套 = 要求核对的路径**（正式 submit
与 `--dry-run true` 两路同点 preflight）。

- 改前 = 基线 `origin/product/main`（分支起点，施工前首跑）
- 改后 = `feature/g3-manifest-absent` 施工完成后（三门全绿后复跑）
- 原始 stdout：`before-stdout.txt` / `after-stdout.txt`

| 场景 | 改前（缺席 = 不核对） | 改后（缺席 = fail-closed 拒） |
|---|---|---|
| **E1 缺席 + 干跑** | `Ok` —— 干跑"成功"，`package_manifest: null`、`checks` 不含 `package_manifest`（**成功但什么都没核对**） | `Err OR1001`：点名 `package_manifest.json` 缺席 + 路径；文案含「G3 fail-closed」「缺席 = 拒」「勿手工拼包」；state 零残留 |
| **E2 缺席 + 正式 submit** | **`Ok` —— 整单真跑完**：签凭证、起引擎（graph 17 turns / 107310 tokens / 210s）、verdict=true、MES 4 事件、账本 8 笔、state 全落盘（破窗最强实证：删 manifest = 绕过内容核对照常下单烧预算） | `Err OR1001`（与 E1 同文案），拒于一切副作用之前，state 零残留，毫秒级失败 |
| **E3 在场 + plan.json 被改（偷删 cost_cap）+ 干跑** | `Err`：`ContentTampered`（plan.json 留档 ≠ 实得 + 「清态换号重提」） | `Err`：**逐字同文案**（判据未动，哈希值都一致） |
| **E4 在场 + 一致 + 干跑** | `Ok`：checks 含 `package_manifest`，evidence 留 manifest 留档（plan_id） | `Ok`：同（判据未动） |

## 文案对照（缺席路径，改后新增的拒绝文案正本）

```
OR1001: 用法错误: OR1001: 任务包缺 package_manifest.json（<task_dir>/package_manifest.json）：
G3 fail-closed——要求核对的路径上 manifest 缺席 = 拒，不放行（未组包或 manifest 被移除均
不可静默当「核对过」；删 manifest 曾可绕过内容核对）；请经 af-compiler 组包
（assemble_construction_package：组包即留档 task/plan 内容 sha256）重产任务包，
勿手工拼包后直接提交
```

改前缺席路径**没有任何报错**——这正是破窗本体：缺席在调用方眼里与「核对过且通过」
同为 `Ok`，区别只剩 `Option` 的 None/Some，不 match 就无从分辨。

## 附：允许路径的显式表达（不适用档）

缺省 stub 档干跑（无 RealExec，manifest 核对不适用）：
- 改前：`"package_manifest": null`（含混——null 既可读成"没核对"也可读成"没这回事"）
- 改后：`"package_manifest": "not_applicable"`（机检可辨，不与"核对过"〔manifest
  留档对象〕混同）
- 集成测试面：`non_verifying_unit_dryrun_reports_manifest_check_not_applicable`
