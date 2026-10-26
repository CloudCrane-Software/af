# ADR-012: 红队修复批（W2）——判分/预算/身份面收口 + 口径勘误

- 状态：已裁决（2026-10-26，红队修复批；依据：红队 W2 波 45 条 real-defect 裁定，裁判亲验）
- 关联：ADR-011（W1 批）、总计划 §0/§2/§6、SYNTAX.md 契约面、RESULT-G2b 偏差表、执行语义规范-v0 §6.5
- 修复载体：分支 phase2-redteam-fixes，commit ff68d80（af-dsl/compiler/execline 十项）/ fa0ec74（execline/order 九项）/ 641bf64（fmt/clippy/词表收尾）/ 208906c（proofs/Rocq/词表）/ 033b847（af 仓文书+索引清理）；九门全绿（step1-8 PASS + step9 ADVISORY-PASS，修复员亲跑 exit 0）

## 背景

红队第二波（W2）对第二阶段交付全景（含 W1 修复面自身）投喂 45 条缺陷候选，裁判裁定全部 real-defect。本 ADR 登记其中需要**决策记录**的部分；逐条修复与测试见上列 commit（TDD 先红后绿）。

## 裁决

1. **判分真值源全链钉死**（C-55/C-52/C-56）：C-59 的 digest 钉死只覆盖 task.json，而 graph/report 两臂全部预算帽在 plan.json——门面 submit 时同钉 plan.json 摘要（凭证身份面留档、重放对账、单元执行前复核三重）；report 臂 R5 判据基准 render.py 可被同目录未锚 .pyc 影子执行——判分目录字节码禁写（-B + PYTHONDONTWRITEBYTECODE）禁读（__pycache__/*.pyc 在场性门）；af-engine 二进制（产出 accept/reject 的那一方）原「存在即用」无锚——建旁车信任锚（sha256 + 来源 workspace），采纳建档/锚不符 `cargo clean -p` 后重建/终核不过 EX1007。
2. **预算面闭环**（C-31/C-54）：token_cap 无 per-launch 收敛且熔断门只在起动前——graph/report 双增加成员完成后第二相熔断，终态超帽不得完工；--max-turns/--spend-cap-cents 此前只 swev 臂消费——接入 graph（订单级总闸 + 节点 turns 缺省）与 report（环级总帽）两臂，订单回显不再是假象。
3. **身份面对账补齐**（C-53/C-37/C-39/C-40/C-41 延伸）：MES event_id 并入运行实例判别量（订单目录规范路径 sha256 前 16hex）——同 order_id 跨 --state 根逐字节同 id 导致差量互吞且零告警；graph_version 期待源由 cli 重读计划的 cfg 值改订单快照不可变绑定——换计划图版本重放从恒真变为可拒；show 位置参数共用 order_id 路径安全门；--spend-cap-cents 负/零值 CLI 前置拒（max_turns=0 零模型探针语义保留）。
4. **L4 逃逸面接线门**（C-30）：计划携带 escape 节点时装订期即拒（点名 impl_ref）——fix_swe_task 的 env_boot 不再以错向 EX3001 失败，编译 exit 0 不冒充 E2E 可执行证据。
5. **af-dsl 语义面**（C-23/C-24/C-25/C-26，承 ADR-011 裁决 6）：E2202 matched 按端口名去重（emits 显式内建端口不重复计数）；has 宏确认状态跨越 // 行注释存活（注释不隔断绕闸）；u64>i64::MAX version 归 E0003；lower is_final 对 from_body 端口经 af_dsl::derived_emit_type 同规则推导且等于 output.ref（非同型不再误标终态）。
6. **执行器/编译器文档口径**（C-27/C-28/C-47/C-51）：RESULT-G2b 偏差表补登 #18-#21（context/consumes/artifact.type/非 1:1 归属）；执行语义规范 §6.5 改口径（仅 node:<path> 投影，plan/loop/map 留计划级段，无 BudgetRef 类型，附 paper_report 实例）；SYNTAX.md 补 E2209/E2202/CEL/default:exit/E2311 语义登记；farm 测试数 13→14。
7. **形式验证披露更正**（C-42/C-43/C-44）：57 分母覆盖率按编号机算（40 有内核载体/12 无直接载体，B-16 计入 FV-0.2 载体，旧 15/26% 作废）；Rocq T05/T06 补零写者前提（CONFLICTS #2/#2b 机器反例）。
8. **仓面卫生与索引**（C-04）：poc-temporal 构建产物 3,191 文件出索引（git 对象复算 5,137,623,292B≈4.78GiB，登记 30-遗产处置；历史过滤待运营者裁决）。
9. **工作区口径勘误**（C-01/C-02/C-03/C-05/C-07/C-08/C-11/C-13/C-18/C-19/C-20/C-45/C-46/C-57/C-58，均为原地文档修正）：逃逸舱占比立可复算口径（97/247=39.3%>30% 未达标，阈值不弱化、处置待运营者）；F4D 在径 verdict=true + 打捞复现 caveat；总计划七门→九门；D1-D10 原文不可核对标记；G3 根级 2→44；F4a 任务/目的错标更正；F-第四单 A/B 去「唯一变量」；/tmp 头寸入观察期硬判据；A7 入预期灯；mes-ttl 取证时序如实；K5 表述限于未合流分支；修复批「随后并入」→待合流；D-2 八单→15 目录；reportkit 设计 19→22 项。

## 理由

- 判分/预算/身份面是可作弊面，收口方向一律 fail-closed 且不留自指校验（期待源必须独立于被验对象）；
- 已发布主线与修复分支的差异必须在文档中显式（K5/合流状态），不得把分支能力写作主线组成；
- 计划项缺定义时先立可复算口径再判定，阈值不弱化（逃逸舱占比即此处理）。
