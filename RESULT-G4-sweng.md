# RESULT · G4 Phase 1 —— 软件工程配置包 sweng v1 施工交付

> 施工批：G4 施工员（单树 phase2-g4，自主线 b3e6866 拉出——裁决16）。2026-10-25。
> 正本依据：《G4-施工裁决-20261025.md》（冲突处以裁决书为准）+ 修订后
> 《G4-软件工程包设计-v1.md》（施工修订 v1.1）。TDD 先红后绿；六位点同步与
> 包落地同 commit；golden 零回归硬线全程未触碰（详见 §5）。

## 0. 一页结论

| 项 | 结果 |
|---|---|
| 交付 | sweng 包 v1：manifest+图（fix_swe_task@1.0）+ impl_ref 锚改写 ×15（pkg.sweng.* 五件包级/pkg.sweagent.* 十件根级，裁决12 落位不动）+ prompts/MANIFEST.sha256 + 15 件真实现（af-execline `pkg/sweng.rs` 进程内注册表）+ template_ref 解析器（裁决10 唯一机制，EX4001-4003 开 EX4xxx 段） |
| E2E 干跑 | `af-compiler-plan --registry af/packages fix_swe_task@1.0.afb` exit 0、stderr 空；**plan_id = `plan.sweng_fix_swe@1.0+535e358b13cb`**（与设计 §4.2 预跑逐字一致；双跑 cmp 逐字节一致）；registry_digest `d5f3bd3a…` → **`d507bf61f6c34a0d90149ab4b99384b6db14bad1285d310e4ee23a4d8400456e`**（57 条不变、impl_ref 锚改写所致；plan_id 不含 digest——逐位复现） |
| 六位点同步 | Phase 1 位点=图计数 14→15（registry_test 严格闭包扩「草稿引用 ∪ G4_TASK_GRAPH_WHITELIST」常量 + bind_test 同步），**与包落地同 commit（b77705f）**；其余五位点（package_count 4×3、组件计数 57×2）Phase 1 不触——第 5 包与 +6 件全部来自 Phase 2 reportkit，届时同 commit 递进至 5/63/16 |
| 测试 | `cargo test -j4 --workspace` **1156 passed / 0 failed**；af-execline 新增 40 单元级用例（35+5）+ 2 真容器抽样 `#[ignore]` 臂 |
| 七门 | fmt/clippy（基线门 0 警告）/test/cue/scrub/cosign/deny = **7/7 PASS，exit 0**（§6 复跑记录） |
| 主线 | b3e6866 未前进（phase2-farm-fixes 尚未并入 phase2，`git merge-base --is-ancestor` 实核）——**无 merge 需求**；farm 批合流后由后续批按裁决 §五-16 复跑终门 |

## 1. 真实现分级明细（裁决17 v1 口径，15 件逐件）

**健康度首报**：E2E 路径组件 2/15 真实现；13/15 为单元级实测（FakeBridge
断言 + 真容器抽样 ≥1 例/件）——此为裁决17 批准的 v1 接受口径；真实模型
执行端到端=铺单批 F-第四单（多 agent），不在本批。

| # | 组件 | 分级 | 实现锚 | 单元级（FakeBridge） | 真容器（anolis） |
|---|---|---|---|---|---|
| T1 | pkg.sweagent.env | **E2E 真实现** | `EnvBash`：`container_exec`（TbEnv::exec 超时参数化镜像）+ `bash_guard_wrap` 护栏原样（反考古/重复 ≥3 拦截） | 命令串黄金值/60s 缺省/护栏三态 | ✓ echo 真执行 exit=0 |
| T2 | pkg.sweagent.aci.search | 单元级 | `grep -rnE -I -- <pat> <path> \| head -n <max>` 自构只读 | 黄金命令串/截断/引号逃逸 | ✓ 命中 django/utils/version.py |
| T3 | pkg.sweagent.aci.open | 单元级 | awk 行号投影；越界/空读 ERROR 观察 | 黄金串/越界/非法区间零容器调用 | ✓ __init__.py:1-3 |
| T4 | pkg.sweagent.aci.edit | 单元级 | python3 载体（JSON argv 直传，old 恰一次才替换） | 载荷可见/0 与 >1 匹配报错 | ✓ 建→改→核 + 0 匹配臂 |
| T5 | pkg.sweagent.aci.create | 单元级 | python3 载体（父目录在/不覆写） | 三态观察 | ✓ OK/已存在 |
| T6 | pkg.sweagent.aci.run_test | 单元级 | `container_exec`（缺省 env_handle 命令、300s、超时显式标记） | 缺省/覆写/超时 | ✓ exit=0 |
| T7 | pkg.sweagent.aci.submit | 单元级 | 首行恒 TASK_COMPLETE + `swev::capture_diff` 定稿 | 哨兵/取证失败不回撤 | ✓ agent.diff 在盘 |
| T8 | pkg.sweng.locate | 单元级 | 同 T2（query 参数面） | 黄金串 | ✓ /tmp 命中 |
| T9 | pkg.sweng.view | 单元级 | 同 T3（start/end 参数面） | 黄金串/缺省 1..100 | ✓ 复核编辑生效 |
| T10 | pkg.sweng.edit | 单元级 | 同 T4 契约 | 载荷 | ✓ hello→world |
| T11 | pkg.sweng.publish_task_finished | 单元级 | node-events.jsonl `node_event(done)` 行 + 工件登记，event_id `{node}/done` 去重幂等 | 双发一行/异节点两行 | （无容器面；真盘实存于单测+抽样臂） |
| U1 | pkg.sweagent.swebench_eval | **E2E 真实现** | `swev::verify` 全链复用（指纹门→capture_diff→eval 容器 1800s→本机 judge）；verdict=judge JSON 原样透传+eval_sha256 注入 | 指纹门负例（零容器调用）+ 装配纯函数 | **✓ 全链**：`{"resolved":false,"log_parsed":true,"f2p_success":1,"f2p_failure":2,"p2p_success":19,"p2p_failure":0,"tests_collected":22,"swebench_version":"4.1.0"}`，eval_sha256 `1bdd01d1…`，三件套在盘 |
| U2 | pkg.sweagent.aci_schema | 单元级 | 纯 serde 结构校验（agent_action 三字段类型面） | 过/缺字段/错型/非对象 | ✓（纯函数件，抽样臂同锁） |
| U3 | pkg.sweng.test_unit | 单元级 | 判定只取退出码+输出末 10 行，零模型 | 四臂 | ✓ SystemExit(3)→pass=false |
| U4 | pkg.sweagent.human_gate | 单元级 | fail-closed 恒 `{ask:false}`（裁决17/开放 4 已批；真工位 af-station 推 v2） | 双配置 | ✓（纯函数件） |

容器纪律：两臂起/清前后 anolis `docker ps -a` 零 `afbench-*` 残留（避让
检查内建于测试——存在在役容器即拒绝起容器）；6g/512 护栏与代理注入随
`swev::setup` 原样。

## 2. 包落地（C2 b77705f）

- `sweng/manifest.json`：+`fix_swe_task@1.0`（组合引用 `pkg.sweng.fix_swe_task@1.0`）。
- `sweng/graphs/fix_swe_task@1.0.afb`：设计 §4.2 图正本**逐字节采用**（104 行，
  sed 自设计文档提取，不经手抄）。
- impl_ref 锚改写 ×15：`executor:builtin/<名>@af-execline`（**无版本段**——版本面
  住声明本身）；name/version/class/effect_class 四字段逐字不动（git diff 仅
  impl_ref 行）；pkg.sweagent.* 十件**维持根级**、pkg.sweng.* 五件**维持包级**
  （裁决12：落位与真实现解耦，sweagent 件归 G4+ 真包化再迁）。
- `sweng/prompts/`：`swev_system.md`（169B）/`swev_instance_protocol.md`（2113B）
  ——与 Rust 冻结常量**字节一致**（pkg_sweng_package.rs 断言 == 常量字节）；
  `aci_system.md`（ACI 面 system 提示，pkg.sweagent.aci_system@1 的解析目标——
  **暂驻 sweng/prompts**：af-dsl 规则「包目录必须有 manifest.json 且 graphs ≥1」
  （registry.rs:230-234），sweagent 无图不可建目录，登记于设计 §3.1）；
  `MANIFEST.sha256`（manifest_version: 1；数据行 sha256sum 同形）。
- README 登记：impl_ref 双形态（裁决13）+ prompts/MANIFEST 机制（裁决10）。

## 3. 六位点同步账（裁决§二，验讫）

| 位点 | G4 前 | Phase 1 后 | 终态（Phase 2 后） | 本批动作 |
|---|---|---|---|---|
| af-dsl registry_test.rs:274 package_count（裁决书 §二 引 :261 为 G4 前锚点） | 4 | **4（不动）** | 5（reportkit） | 无 |
| af-dsl registry_test.rs:290-303 declared_graphs（裁决书引 :271-277 为前锚点） | 14 | **15**（+fix_swe_task） | 16（+paper_report） | 扩 `G4_TASK_GRAPH_WHITELIST` 常量（草稿 ∪ 白名单），**同 commit b77705f** |
| af-compiler bind_test.rs:73 package_count | 4 | **4（不动）** | 5 | 无 |
| af-compiler bind_test.rs:81 图计数 | 14 | **15** | 16 | 同 commit 同步 |
| af-compiler bind_test.rs:44 组件计数 | 57 | **57（不动）** | 63（+6 reportkit） | 无 |
| af-compiler registry_decl_test.rs:144 组件计数（裁决书 §二 误标 af-dsl——实位 af-compiler/tests） | 57 | **57（不动）** | 63 | 无 |

（行号锚点为 G4 后实位——独立复核 2026-10-25 亲测；本表写作时点引用的
:261/:271-277/:66/:68 为当时锚点，数值与结论不变。）

实跑证据：`cargo test -p af-dsl -p af-compiler` 302/0；`cargo test -j4
--workspace` 1156/0。图闭包「注册表卫生语义」保留——白名单常量形式，清单
仍不得夹带闭包（草稿 ∪ G4 任务图）外图。

## 4. E2E 干跑与 digest 联动（裁决8）

- 干跑（判据 0）：见 §0；glm-cp 档为 E2E 执行基线（裁决11/18，经单元 spec
  覆写兑现——执行段属铺单批 F-第四单；本批干跑不涉模型）。
- **15 稿逐位一致性（已验）**：15 份对齐草稿以本批注册面全量重跑
  （`af-compiler-plan --registry af/packages`，15/15 exit 0），与
  `计划/研究组/对齐计划快照/` 各快照 diff：**每份恰 `compiled_from.registry_digest`
  一行变化**（`d5f3bd3aa0f7ea65…d57977` → `d507bf61f6c3a0d…4456e`），plan_id
  与其余字段逐位相同（plan_id 不含 digest，G3 §① 先例复现）。
- **快照文件与 N3 登记表的物理更新**：暂缓至 G4 终态（Phase 2 reportkit +6 条
  后 digest 再变）一次到位，避免正本两连改；本 RESULT 即裁决8 要求的逐位一致性
  登记载体（Phase 1 中间值 d507bf61… 已在案）。**待办移交**：Phase 2 收官时
  更新 15 稿快照 digest 行 + 中立性 N3 登记表联动（方案 §3.2/§5-5）。

## 5. golden 零回归与任务中立

- golden 面：21 份 IR golden 与 4 份 selector golden 全量复测通过（workspace
  1156 内含）；impl_ref 不入计划投影（PlanComponents 只携带
  reference+effect_class）——15 稿重跑实证「计划体积/plan_id 不变」（§4）。
- 核心零改动：af-dsl/af-compiler src **零行改动**（本批 diff 只触 tests 的两处
  计数同步）；全部实现住 af-execline 包内新模块 `pkg/`（中立性方案 §2.2 执行面
  豁免范围）。
- EX4xxx 段开段（ADR-007 数字只增）：EX4001 模板引用非法/EX4002 未登记/
  EX4003 漂移；**E9004→EX4004 码位预留给 Phase 2 planbind**（裁决9 唯一正本）。

## 6. 七门复跑记录

`bash ci/run.sh` @ 86dde40（本批 HEAD）：**7/7 PASS，exit 0**——step1 fmt /
step2 clippy+基线门（0 警告 0 签名）/ step3 `cargo test -j4 --workspace`
1156/0 / step4 cue / step5 scrub / step6 cosign 断网验签 / step7 cargo deny。
产物 `ci/out/`（SUMMARY.txt 随批入档）。

环境注记（如实）：新工作树预检首跑缺 `bin/cosign`——`/bin` 在 .gitignore
（本地资产不入库，主线树同款），自主线工作树拷入同一二进制后通过；此为
工作树本地补齐，非仓内容变更。首跑预检失败留痕 `ci/out/step0.log`。

## 7. 范围外与待办（如实）

1. **planbind.rs 通用装订器**（E9004→EX4004 执行面消费唯一正本，裁决9）：
   裁决 §七列 Phase 2（reportkit 批含 planbind/--unit/ReportExecuteUnit/
   cite_check）；本批已交付其模板解析半体（prompts.rs）与 EX4xxx 段位。
2. **--unit swev\|report\|stub 路由**（裁决7）：Phase 2 批 C。
3. **中立性门 step8**（ci/neutrality_gate.sh 等六件 + 挂入 run.sh）：Phase 3；
   本批保持 run.sh 七门原样。
4. **真实模型执行端到端**（判据 3/4/8 真模型段、MES 真写）：铺单批 F-第四单。
5. **15 稿快照 digest 行物理更新 + N3 登记表联动**：见 §4（G4 终态一次到位）。
6. 词表再生成+重收敛（裁决19）：reportkit 6 名入册后（方案 §5-8）。

## 8. 提交清单（本批）

| commit | 内容 |
|---|---|
| fd788d3 | feat(execline): 15 件真实现 + template_ref 解析器（EX4xxx 开段）+ SwevUnitSpec 装载化 + 35 单元级用例（先红后绿） |
| b77705f | feat(packages): sweng 包 v1 落地 + impl_ref 锚改写 ×15 + prompts/MANIFEST + 六位点同步（同 commit）+ 5 包资产用例 |
| 86dde40 | test(execline): 真容器抽样实跑修正与判分留证 |
| （本件） | RESULT-G4-sweng.md + ci/out 七门产物 |

分支 `phase2-g4` → `origin/phase2-g4`（推送即达）。
