# poc-admit-seal-roles — admit_seal 补 `--decision` / `--role` 面

> **要补的洞**：`crates/af-order/examples/admit_seal.rs`（steward 侧准入签名联调
> 工具）此前硬编码 `Role::Approver` + `Decision::Approve`——只能签 Approver×Approve
> 一种组合。BOOT-2（af 自举链）当时为了签一份 **Reject** 裁决，只能临时造一个联调
> 样例用完即删、未入分支。本批把 `--decision <approve|revise|reject>` 与
> `--role <approver|reviser|arbiter>` 落进工具面，配对合法性在**签出之前**判，
> 并为 Reject / Revise / Arbiter 三条路径补齐正例与负例测试 + 真跑留证。
>
> 纯补参，零新机制：不新增 crate（`crates/` 集合与根 `Cargo.toml` members 未动）、
> 不新增 OrderType 枚举、不造第二套权限阶梯——配对判据直接落在
> `af-workorder/src/decision.rs` 既有权限面之上。

## 1. 改了什么、为什么

| 件 | 改动 | 说明 |
|---|---|---|
| `crates/af-workorder/src/decision.rs` | `impl Role` 增 `pub fn can_decide(self, Decision) -> bool` + 配对矩阵单测 `role_decision_pairing_matrix_matches_permission_face` | 判据一行：`role.allowed_permissions().contains(&decision.required_permission())`——与四层校验第③层（决策权限）同一判据、同一张权限表，不是另造阶梯。签发侧用它把非法配对拦在签出之前；受理侧同判据兜底 |
| `crates/af-order/examples/admit_seal.rs` | 加 `--decision` / `--role` 两参数（严格小写取值，缺省保持现行为 Approver×Approve，**默认语义未动**）；配对门置于一切读取/签出之前；`TrustedActorRegistry` 的 actor 角色与 `DecisionArtifact::seal` 的角色同取 `--role`；stdout 留证增 `role`/`decision`/`permissions` 三面（取自签出的工件本身） | 非法配对 = 中文报错 + exit 2 + **未读账本、未写任何文件**。不放行到受理侧才红，也不产出自相矛盾的请求（工件角色 ≠ 注册表角色的形态工具面产不出） |
| `crates/af-order/tests/admit.rs` | 新增 3 正例 + 3 负例（既有 13 用例与断言**一字未动**，全部只增） | 见 §3。负例一律断言 `admissions.jsonl` 不存在（状态树零写入） |

**配对矩阵**（判据 = `Role::allowed_permissions` × `Decision::required_permission`，
单测钉死）：

| role \ decision | Approve | Reject | Revise | Arbitrate |
|---|---|---|---|---|
| Reader（只读，不开放） | ✗ | ✗ | ✗ | ✗ |
| Approver | ✓ | ✓ | ✗ | ✗ |
| Reviser | ✗（无 Approve 权） | ✗（Reject 同需 Approve 权） | ✓ | ✗ |
| Arbiter | ✓ | ✓ | ✓ | ✓（只有 Arbiter 能 pivot） |

工具面只收 `approve|revise|reject` × `approver|reviser|arbiter`：`arbitrate`
（pivot）归裁决环 `run_steward_loop` 生产面；`reader` 对任何决策都无权限，开放
只会产出必然被拒的工件。

## 2. 签出的工件字段怎么随 `--role` / `--decision` 变

`DecisionArtifact::seal` 既有口径不变：`permissions` 恒取角色默认权限集
（`role.default_permissions()`），`role` / `decision` / `permissions` 与四扩展
字段、payload 同进被签字节。所以：

- `--role approver` → 工件 `role:"Approver"`，`permissions:["Read","Approve"]`
- `--role reviser`  → 工件 `role:"Reviser"`，`permissions:["Read","Revise"]`
- `--role arbiter`  → 工件 `role:"Arbiter"`，`permissions:["Read","Approve","Revise","Arbitrate"]`
- `--decision`      → 工件 `decision` 与回执 `decision`（serde 名）同取所选

受理侧 `af-order admit`（OR21xx 全链，第③步真引擎子进程四层验签）对每一份
工件的结论与所选 decision 相符（见 §4 回执 `decision` 字段）。

## 3. 测试面（tests/admit.rs 新增 6 例 + decision.rs 配对矩阵 1 例）

| 用例 | 路径 | 断言要点 |
|---|---|---|
| `admit_accepts_approver_signed_reject_and_records_decision` | Reject 正例 | 工件 role/decision/permissions 与所选一致；真引擎全链受理；`receipt.decision=="Reject"`；admissions.jsonl 恰 1 行 |
| `admit_rejects_reviser_signed_reject_with_or2106_and_zero_writes` | Reject 负例 | Reviser×Reject 越权 → OR2106（拒因含「决策权限不足」）；**admissions.jsonl 不存在** |
| `admit_accepts_reviser_signed_revise_and_records_decision` | Revise 正例 | `receipt.decision=="Revise"`；permissions=[Read, Revise] |
| `admit_rejects_approver_signed_revise_with_or2106_and_zero_writes` | Revise 负例 | Approver×Revise 越权 → OR2106 + 零写入 |
| `admit_accepts_arbiter_signed_reject_then_revise_with_monotonic_seq` | Arbiter 正例 | 同源单连续受理 Arbiter×Reject(seq1) + Arbiter×Revise(seq2)；permissions=[Read,Approve,Revise,Arbitrate]；回执 decision 逐一相符；账本 2 行 |
| `admit_rejects_claimed_arbiter_not_backed_by_registry_with_or2106_and_zero_writes` | Arbiter 负例 | 工件声称 Arbiter、注册表只登记 Approver（签名虽真）→ 四层第④层 OR2106（拒因含「角色越权」）+ 零写入 |
| `role_decision_pairing_matrix_matches_permission_face`（decision.rs 单测） | 矩阵 | 全 4×4 配对逐一断言；`can_decide=false` 的每个配对再走一遍 `validate_against` 钉住「四层第③层同判据拒绝」 |

工具面（admit_seal 自身）的非法配对拒绝不在 `cargo test` 面上（example 无测试
靶位），由 §4 真跑留证覆盖（exit 2 + 零写出 + 报错全文）。

## 4. 真跑一次：证据索引（run/ 下，相对本目录）

运行：2026-11-04T03:30:06Z，anolis-gpu-01，工作树 = product/main(83bb71c) +
本分支未提交改动（`run/run-meta.json` 记 HEAD 与 rustc 1.99.1；改动文件清单见
`run/logs/git-status-poc.txt`）。复现：

```bash
export PATH="$HOME/.cargo/bin:$PATH" LANG=C.UTF-8
CARGO_TARGET_DIR=/data/bench/af-target bash poc/poc-admit-seal-roles/run_poc.sh
```

| 步 | 证据文件 | 关键读数 |
|---|---|---|
| ⓪ 源单 submit | `run/logs/00-submit.out`、`run/spec.json`、`run/state/orders/wo-asr-poc-1/{ledger.jsonl,mes-events.jsonl}` | 真实 CLI submit（stub 单元、真引擎门控）全生命周期：state=Closed、账本 8 行、MES 4 事件、stable_id=sid-1524d728…8d50 |
| ① Reject 路径 | `run/admissions/reject-approver.json`、`run/logs/seal-reject-approver.out`、`run/logs/admit-reject-approver.out` | `--decision reject --role approver --seq 7`：工件 role=Approver/permissions=[Read,Approve]；回执 decision=**Reject**、11 checks、engine 0.2.0 |
| ② Revise 路径 | `run/admissions/revise-reviser.json`、`run/logs/{seal,admit}-revise-reviser.out` | `--decision revise --role reviser --seq 8`：工件 role=Reviser/permissions=[Read,Revise]；回执 decision=**Revise** |
| ③ Arbiter 路径 | `run/admissions/reject-arbiter.json`、`run/logs/{seal,admit}-reject-arbiter.out` | `--decision reject --role arbiter --seq 9`：工件 role=Arbiter/permissions=[Read,Approve,Revise,Arbitrate]；回执 decision=**Reject**（幂等键 `sid-1524…/admit/9/581c051b…`） |
| ④ 缺省回归 | `run/admissions/default-approve.json`、`run/logs/{seal,admit}-default-approve.out` | 不给两参数 = 旧行为 Approver×Approve（seq 10）照常受理——默认语义未变的对照证据 |
| ⑤ 受理账本 | `run/state/orders/wo-asr-poc-1/admissions.jsonl` | 4 笔，seq=[7,8,9,10] 单调，decision=[Reject,Revise,Reject,Approve]，source_state=Closed |
| ⑥ 非法配对拒绝 | `run/logs/neg-approver-revise.{err,rc}`、`run/logs/neg-reviser-reject.{err,rc}` | exit 2；`非法配对：--role approver 不能签 --decision revise——Revise 所需权限 Revise 不在 Approver 角色的权限面 [Read, Approve] 内（根本法角色阶梯 …）。拒绝签发：未读账本、未写任何文件`；`--out` 目标文件**未创建** |
| ⑦ 非法取值拒绝 | `run/logs/neg-decision-arbitrate.{err,rc}`、`run/logs/neg-role-reader.{err,rc}` | exit 2；`--decision 只接受 approve|revise|reject`、`--role 只接受 approver|reviser|arbiter（… reader 只读 … 不开放）` |
| ⑧ 独立对账 | `run/verify.out`（`run_poc.sh` 内嵌 `python3 -I` 校验） | 四份工件 role/decision/permissions/注册表角色逐项相符、四份回执 decision 相符且各 11 checks、账本 4 笔序列核对——`VERIFY-ALL-PASS` |
| ⑨ 清单 | `run/evidence-sha256.txt` | run/ 下 50 文件 sha256（`voucher-key.seed` 为密钥材料，不入清单不入库，见 `.gitignore`） |

真跑命令原文（步①为例，其余同形换参数）：

```bash
/data/bench/af-target/debug/examples/admit_seal \
    --state run/state --order-id wo-asr-poc-1 --actor-id user-a \
    --expires 2027-01-01T00:00:00Z --next-orders wo-asr-poc-1-rw \
    --seq 7 --seed-hex 11…11 --decision reject --role approver \
    --out run/admissions/reject-approver.json
/data/bench/af-target/debug/af-order admit \
    --request run/admissions/reject-approver.json --state run/state \
    --workspace /home/pan-ding/af-dev/af --target-dir /data/bench/af-target \
    --now 2026-11-04T00:00:00Z
```

（`--now`/`--expires`/`--seed-hex` 钉死，跨跑可复现；`admitted_at` 即 `--now`。）

## 5. 门读数（本分支工作树，cargo 1.99.1 / rustc 1.99.1）

| 门 | 命令 | 读数 |
|---|---|---|
| 测试 | `cargo test --workspace` | **exit 0；149 个 suite 全 ok；合计 1481 passed / 0 failed**（跑了两遍：共享 target 一遍 + 独占 target `CARGO_TARGET_DIR=/data/bench/af-target-asr` 无并发一遍，两遍计数相同） |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0**（零警告） |
| clippy 基线门 | `cargo clippy --workspace --all-targets --message-format=json` + `python3 -I ci/clippy_gate.py <jsonl>` | **PASS——现有 0 条警告 / 0 签名，均在基线内**；`ci/clippy-baseline.txt` 一字节未动 |

分 crate 复核：`cargo test -p af-workorder` 71/0（含新增配对矩阵）；
`cargo test -p af-order` 95/0（含新增 6 例 admit 集成用例；admit 套件单跑 3 次均 19/0）。

## 6. 边界（做不到的、需要人类裁决的）

1. **`arbitrate` / `reader` 不进本工具面**（有意收窄，非遗漏）：`arbitrate`（pivot）
   是 Arbiter 专属第四种决策，其消费方是裁决环 `run_steward_loop` 的
   continue/rework/abort 分支，不归联调签发面；`reader` 对任何决策都无权限。
   若要把 `--decision arbitrate` 开进 admit_seal，需人类裁决是否让联调工具面
   触碰 pivot 面（根本法：只有 Arbiter 能 pivot）。
2. **工具面产不出「工件角色 ≠ 注册表角色」的请求**：admit_seal 的注册表与工件
   同取 `--role`，这是联调形态的简化（自造单 actor 注册表 + 一次性密钥）。该
   冒升形态的受理侧兜底由集成测试
   `admit_rejects_claimed_arbiter_not_backed_by_registry_with_or2106_and_zero_writes`
   钉死（OR2106 第④层），但**生产面**的多 actor 注册表装载/轮换不在本批。
3. **Reject/Revise 的后续效果不在本批**：admit 受理链只留档回执（真跑里源单
   保持 Closed、四笔回执入账）。abort 源单（Reject）/物化 rework 单（Revise）
   是 `adjudicate.rs` steward 闭环的职责，本批未动、也未真跑该闭环。
4. **工具面「零读取零写入」是顺序性保证**（配对门在一切 fs 访问之前），没有做
   文件系统级快照对账；受理面零写入有 `admissions.jsonl` 不存在断言钉死。
5. **一次未复现的瞬时测试红**（如实登记）：本批中途一次 `cargo test -p af-order`
   出现 1 例 admit 套件失败（当时输出未留存，具体用例不可考）；随后 6 次复跑
   （admit 套件 ×3、af-order 全量 ×2、workspace 全量 ×1）均 0 failed，最终又以
   **独立 target 目录**（`/data/bench/af-target-asr`，与兄弟批次零共享）完整重跑
   `cargo test --workspace` 一遍：149 suite / 1481 passed / 0 failed。环境因素
   可疑：本机与并行的兄弟批次（wt/compiler-diagnostics、wt/mainline-fmt-zero
   工作树）**共用** `CARGO_TARGET_DIR=/data/bench/af-target`，且本机有 OOM 史
   （曾见 0 字节 rmeta 竞写痕迹与一次 stale-rlib 假错，touch 重编后消失）。
   后续批次复跑门时，建议换独立 `CARGO_TARGET_DIR` 或避开兄弟批次 cargo 时段。
6. **admit_seal 的 stdout 留证面加了三个字段**（`role`/`decision`/`permissions`，
   取自签出工件）：仓库内无消费者钉该 stdout 形状（已核），但若外部有脚本解析
   此行需知悉为**只增不改**。
