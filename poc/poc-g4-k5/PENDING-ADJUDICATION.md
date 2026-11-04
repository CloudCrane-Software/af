# poc/poc-g4-k5 —— G4 返修批待裁决登记件（PENDING-ADJUDICATION.md）

- 登记批：G4 返修批（2026-11-04，复核判不可收卷后的 F1/F2/S13/F3 落地）
- 性质：**只登记、不改判据、不改 registry**。以下两项按复核 `REVIEW.md`
  fixes F3 列为「后续裁决批」事项；本件给出事实、技术分析与**建议**——
  **建议≠裁决，裁决权在主代理/运营者**。

---

## 裁决项 ①：MUT-007 description 陈旧指针（P3，轻微留痕）

**事实（现指什么 / 实际应指什么）**：

- 现状：`mutations/registry.yaml` MUT-007 description 末段写
  「……修复后由『判分链必需成员下界』对账（**ci/k5-path-lint.py
  REQUIRED_MEMBERS**）与『门输入全集钉』（ci/gate_inputs_check.sh GATE_INPUTS
  含 ci/scorechain-manifest.txt）双臂拦截」。
- 陈旧点：G4（`f01bb99`）起 `REQUIRED_MEMBERS` 手工元组已被
  **仓面派生**取代（`derive_required_members()` = report/swev 两臂现算并集），
  该常量已不存在；门件收缩红文案尾句也已改指「本文件下界派生规则」。
- 应指：同句改为「ci/k5-path-lint.py 下界派生规则
  （derive_required_members 仓面现算）」之类的现行机制名即可（措辞由裁决批定）。
- 影响面：**仅描述文本**。MUT-007 的行为期望（exit 1 + stderr 含
  「判分链登记表被收缩」）在改后门上仍成立——S1 沙箱原样重放改前改后双中
  （README §5.5），MUT-001/007 探针无行为回归。

**建议**：后续裁决批顺手更新该 description 指针（同款先例=本仓已修的收缩红
文案指针、门族面 ref 自派生等指针随机制更新）；无需单独批。

## 裁决项 ②：是否恢复 R5 类形态的 k5 单门双层拦截（P1 的裁决面）

**问题**：R5/R5bak（删成员文件+删登记行同批，部分臂协同收缩）在 k5 单门
改后不红（防线 2 层→1 层，实际拦截者=step0 表钉；实证 README §4.1 S13）。
是否要让 k5 单门也拦住该形态（例如本门自钉 manifest 摘要、或下界加手工
计数/清单常量）？

**技术分析**：

1. **与 step0 表钉职责重叠**：R5 形态**必然触碰** `ci/scorechain-manifest.txt`
   （删登记行），而该表已被 `gate_inputs_check`（run.sh step0，
   `ci/baselines/gate-inputs.sha256`）逐字节钉死——同批表改动必红 exit=1
   （S13 三形态亲证）。k5 再自钉同一张表 = 同一锚在两处维护：每次**合法**
   换锚（史实：该表 2026-10-27 有 W2-C-51→W3-C-34、W2-C-50→W3-C-06 等多次
   裁决换锚）都要同批改两处，漏一处即在**正道**上误红——新增的摩擦与误红面
   恰好落在合法演化路径上。
2. **自钉的对抗强度是假的，除非门件本身被钉**：k5-path-lint.py 目前**不在**
   GATE_INPUTS 钉面（.py 非判据类扩展名）——若把 manifest 摘要钉进 k5 门件
   常量，攻击者同批改 k5 常量即绕过（门件自身无钉），并未真正加厚；要真加厚
   须同时把 k5-path-lint.py 纳入钉面+update 纪律——那是机制面扩钉（改
   GATE_INPUTS 清单与钉文件），超出「加一行常量」的量级，本就须裁决。
3. **手工计数/清单常量会复活 G4 已消灭的病**：下界若退回手工常量（件数或
   名单），即回到「只钉 N 件」同型漏钉（RT-E6/E51 教训）：换件不换数=漏，
   新成员忘 bump=正道误红；G4 改派生正是为消掉这个面。
4. **派生面自带复装**：被删成员若日后重新落盘（或包内新增 .py），自动重入
   派生集、不登记即红（S6 型）——收缩窗要保持住就得持续压着 manifest，
   而 manifest 被 step0 钉死，管道级出不去。
5. **单门语义的真实缺口有多宽**：k5 lint 在管道内由 run.sh step1 消费，
   step0 恒先于 step1——管道内不存在「绕过 step0 单跑 step1」的路径；缺口
   仅存在于**脱离管道手工单跑** k5 lint 的场合（无消费点这样跑）。

**建议（供裁决，非裁决）**：**维持现状**——以 F1/F2 落下的边界写法为准
（单门防线如实记为 2→1，管道级无逃逸由 step0 表钉拦），不在 k5 门内加自钉
或手工常量。若运营者评估认为「脱离管道的单门单跑」是须防的对抗面，正确
形态是独立裁决批：把 k5-path-lint.py 纳入 GATE_INPUTS 钉面（或等价的
manifest 摘要外钉）+ 同批更新 MUT-007 description（裁决项①可搭车），
一次把「钉什么、怎么 update、误红面」整体裁决清楚。

---

## 附：本批与裁决批的分界（自查）

- 本批已做：说法修正（`ci/k5-path-lint.py` 仅注释 +11/−3；README §3/§4/§5/§8）、
  S13 补证（新造夹具，`run-s13.py`）、本登记件。
- 本批未做（留裁决）：改 `mutations/registry.yaml`、在门内加任何拦截层、
  扩 GATE_INPUTS 钉面、动 `ci/` 其它件或 `crates/` 任何件。
  判据面零改动机械证明：`evidence/noncomment-bytecheck.log`。
