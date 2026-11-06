# changelog-draft · E9001 组件注册供给批

- 主代理追加口径：2026-11-05 波次 E9001（行首时间以 计划/变更日志.md 实际追加为准）。

## 施工

1. 基线 fetch `origin/product/main` = `bb7d1e3`；新分支 `feature/af-dsl-component-registry-e9001`。
2. seam `compile-all . ./af/packages` 实跑：10/44 过全链，34 条断 bind（E9001）。
3. 选 a：注册表 +56（route 7 / tool 30 / unit 19），`impl_ref` 指向已接线 builtin 或真实端点；`tool_impl_name` 把新 tool 名落到 env/locate/view/edit 四臂。
4. 选 b：st-C122 / st-b-C079 卸 `pkg.fs.workdir_rw` / `pkg.net.proxy_allowlist`；st-b-C042 `uses.memory` 的 `pkg.mem.*` 降为 `pkg.sweng.locate/view/edit`。
5. 计数测试 63→119；digest 钉改为与 live registry 对账。

## 机械验证读数

- seam 四段链：10/44 → **44/44**
- `python3 -B ci/strategies_check.py`：exit 0，44/0/17
- `cargo test -j4 --workspace`：1620/0/9（基线 1617/0/9，+3 本批断言）
- fmt / clippy `-D warnings` / clippy_gate PASS / gate_inputs 14 件一致

## 边界

- unit 别名共用 `pkg.sweng.test_unit` 真实现（语义是判定单元，不是各自领域的独立判据器）。
- tool 别名共用 locate/view/edit/env 四臂（merge 策略工具落 edit；lint/test/runtime 落 env）。
- store retrieve 的 `component:` 字段 bind 不核（pkg.skill.loader / pkg.ir.bm25_ce / pkg.mem.grep 仍在 snippet 内）。
- p3_judge 在带 `BASH_FUNC_which%%` 的交互 shell 下会红；`env -u` 后 5/0/1。
- 未改判据面；未新建空包目录。

## 返修（minimax 复核不可收卷 → grok 通道，2026-11-05）

1. **M1 凭证明文出仓**：`git rm` `raw/gate-test.workspace.log` / `raw/gate-test.p3_judge.log`（上批 ssh 会话带代理跑测，env dump 把凭据打进日志）。干净 env 重生 workspace 日志：`env -u` 六代理变量 + `BASH_FUNC_which%%` + `LC_CTYPE=C.UTF-8`；落盘前对代理用户名/口令前缀/代理地址三元组 grep 自检 = **0**。读数 **1620/0/9**（与 README 声称一致）。
2. **M2 golden digest 字面量钉**：`pkg_reportkit_package.rs` 撤回 `assert_eq!(digest, reg.digest())` 恒真，改回复核员公布字面量 `f5a6de7c9faa7859ab2b8c206a7517b996e770a78099baea9ac7cd1e565f7103`。`cargo test -p af-execline --test pkg_reportkit_package` **5/0**。
3. **F1** `st-b-C042.yaml` store retrieve `component: pkg.mem.grep` → `pkg.sweng.locate`（已注册等价；bind 不核 context.component，但仍不许留空转件名）。
4. **F2** `all_e9001_declarations_resolve_with_builtin_impl_ref` 登记进 `ci/neutrality-expected-fail.txt`（23→24）；`numbers.json` `redlist.count` / `doc-claims.txt` / `gate_inputs` 钉同步。`bash ci/neutrality_gate.sh`：隔离臂 FAILED **31**（af-dsl 2 + af-compiler 29）= 基线 30 + E9001 新增 1；I2 通过（24 项均在失败集）；I1/I3 仍红于 G2/G3 预存 7 项（`construction_build_test`×3 / `construction_teaching_test`×3 / `diagnostics_cli_test`×1）未入名单；S1 红于 `construction.rs` 预存领域词 7 处——均非本批引入，本批不扩面收预存债。
5. **F3** `st-b-C079.yaml` intent 同步：uses.tools 只列 `pkg.repo.grep@1`，不再声称 `pkg.net.proxy_allowlist` 在 uses.tools（能力面仍在 escape impl 参数）。

### 返修门读数

- seam 四段链：**44/44**（C042 plan_id `…4f7ebc750547` → `…36d23ae00648`；C079 `…5270898870c5` → `…eeec2db2e20d`）
- `python3 -B ci/strategies_check.py`：exit 0，44/0/17
- `cargo test -j4 --workspace`：**1620/0/9**（凭据 grep 零命中）
- fmt / clippy `-D warnings` exit 0 / clippy_gate PASS（0 签名）/ gate_inputs 14 件一致
