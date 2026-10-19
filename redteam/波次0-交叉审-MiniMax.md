# 波次 0 文档-现实一致性交叉审（MiniMax）

- 审查时间：2026-10-19 15:38–15:55 CST
- 审查者：红队交叉审 · MiniMax（只读审查，视角=数据一致性与状态文档真实性）
- 审查对象：波次 0 变更涉及的三份文档声明 + af 仓库提交 + 遗产处置救援齐套性
- 运行环境：本机（100.64.0.3 侧，home 在 /dev/nvme0n1p2 468G）+ anolis（100.64.0.7，**ssh 可达，本轮已实测**，非"无法核实"）

## 结论摘要

| # | 检查项 | 结果 |
|---|---|---|
| 1 | 变更日志.md 今日 3 条声明的证据 | 2 条证伪 / 1 条证实（见 F1、F2、F12） |
| 2 | 组件建设状态.md ✅ 标记 vs 实际 | 7 项证实 / 4 项不符（F3、F4、F5、F6） |
| 3 | af 提交 4dc1040 只含格式化 | **证实**（但 ± 对称性检查不成立，见 F7 说明） |
| 4 | 30-遗产处置 删除记录 vs 参考目录 rescue | 齐套（ADR 15+INDEX、任务1 报告 3 份、设计快照）✅；但删除记录本身漏登 anolis 大删（F2） |

---

## F1（high）变更日志.md:8 "④下载清单 8/8 ✅（Bifrost 源码…）" — Bifrost 源码克隆实为损坏状态

**声明**：`计划/变更日志.md:8` 第 8 行，"④下载清单 8/8 ✅（Bifrost 源码/CH 双机/OTel 重建/rekor 双变体/docker 快照/bin 核对/远端）"。

**现实**：`~/外部组件/bifrost/` 只有一个 `.git`，无工作树、无任何提交，HEAD 指向 `refs/heads/.invalid`，且残留 `.git/shallow.lock`。git 明确报错分支损坏。

**复现**：
```bash
ls -a ~/外部组件/bifrost/          # 只有 .git
cd ~/外部组件/bifrost && git log --oneline
# fatal: 您的当前分支似乎已损坏
cd ~/外部组件/bifrost && ls .git/*.lock
# .git/shallow.lock
cd ~/外部组件/bifrost && cat .git/HEAD
# ref: refs/heads/.invalid
```

**要点**：同一次变更里 `状态/组件建设状态.md:28` 把 Bifrost 标为 "🔧 进行中（源码克隆中）"——两份文档对同一资产给出互相矛盾的结论，append-only 的变更日志反而写了更强的说法。按"唯一权威"纪律（组件建设状态.md:3），变更日志 8/8 ✅ 的口径是假的。

---

## F2（medium）anolis 11.3G 删除未进 30-遗产处置 删除记录，且 30 号文件仍写"不动 / DoD 挂起"

**声明 A**：`计划/变更日志.md:8` "①anolis 根分区 93%→74%（运营者授权删 agentic-factory-projects 11.3G）"；`状态/机器清单.md:36` 同口径删除记录。

**声明 B（冲突）**：`计划/30-遗产处置.md:33` 仍写 `/home/anuser/agentic-factory-projects` 处置为"**不动，登记观察**"；`:40` 仍写 "**波次 0 DoD「anolis 根分区 <80%」现实不可达**…本 DoD 项挂起，不算完成"。

**现实**：`30-遗产处置.md` 的 §3 删除记录表（:59-63）共 3 行，**全部是本机条目，没有任何 anolis 行**。该文件第 4 行声明"每批删除前在本文件'删除记录'表登记"，而一次 11.3G 的 anolis 删除没有登记。变更日志 line 9 还专门写了一条"达成方式变更"来说明裁决，却没回头改 30 号文件——30 号文件仍停留在裁决前的口径。

**已核实的部分**：`DoD anolis 根分区 <80%` 客观达成 —— 实测 anolis 根分区 75%（`/dev/vda3 98G 70G 25G 75% /`），<80% 成立。但"11.3G 已删"本身**无法核实**：`/home/anuser` 对 pan-ding 是 `Permission denied`，anolis 上 `sudo -n` 需要密码。

**复现**：
```bash
grep -n "不动，登记观察\|现实不可达\|挂起" 计划/30-遗产处置.md
sed -n '57,64p' 计划/30-遗产处置.md          # 删除记录表只有本机 3 行
timeout 30 ssh -o BatchMode=yes anolis 'df -h / | tail -1; ls -d /home/anuser/agentic-factory-projects; sudo -n true'
# /dev/vda3  98G  70G  25G  75% /
# ls: cannot access '/home/anuser/agentic-factory-projects': Permission denied
# sudo: a password is required
```

---

## F3（medium）变更日志.md:8 "af 基线绿 + T1 fmt 清零" 与 组件建设状态.md:26 "fmt 门红=预存 235 块" 自相矛盾

`状态/组件建设状态.md:26` 写 `CI 六门（af/ci/run.sh）| ✅ 建成，5/6 绿 | fmt 门红=预存 235 块，波次 1 清零`，而同文件 `:44` 写 `T1 fmt 全量格式化 | 本机 GLM5.3 | ✅ 4dc1040（39 文件，174+1 绿，CI fmt 门转绿）`。同一文档相隔 18 行，一处说 fmt 门红、一处说转绿，`:26` 是陈旧残留。

**fmt 门客观已绿（本轮实测）**：
```bash
cd ~/workspace/agent黑灯工厂/af && source ~/.cargo/env && cargo +1.99.1 fmt --check --all; echo "EXIT=$?"
# 无输出，EXIT=0
grep -n "step1 cargo fmt" af/ci/run.sh    # ci/run.sh:75 六门中的第 1 门
```

---

## F4（medium）T9 已有两个提交落地，文档仍标 ⬜ 待做；且两提交未推 gitea

`状态/组件建设状态.md:51` 写 `T9 cargo 阶梯接入 | 本机 GLM5.3 | ⬜ 待做`。实际 af 仓库在 4dc1040 之后已有两个 T9 提交：
- `5c745d2 T9a: rust-toolchain 钉 1.99.1 + deny.toml 首发最小集（波次 1 ⑦部分）`
- `6bd03f7 T9b: advisory-db 本地化（11M 克隆入 ci/，gitignore 排除）+ 刷新脚本`

`计划/变更日志.md` 也没有 T9a/T9b 的任何条目（最新一条 line 8 只写到 T1）。且这两个提交**未推送**：本地 main 领先 origin/main 2 个提交，gitea 上 main 仍停在 4dc1040。

**复现**：
```bash
cd ~/workspace/agent黑灯工厂/af && git log --oneline -4
# 6bd03f7 T9b / 5c745d2 T9a / 4dc1040 fmt / 2e06908 仓库迁入
git status -sb | head -1              # ## main...origin/main [领先 2]
git ls-remote origin                  # refs/heads/main = 4dc1040...
```

---

## F5（low）组件建设状态.md:5 "git 60f6861" 与 组件建设状态.md:26 "5/6 绿" 均为陈旧基线

`:5` 头部写 `当前基线 174+1 测试全绿，git 60f6861`，`:62` 更新记录同样停在 60f6861；仓库实际 HEAD 已是 6bd03f7。属 append 纪律执行不到位（低危，指向正确但会误导接手人）。

```bash
grep -n "60f6861" 状态/组件建设状态.md
git -C af rev-parse --short HEAD      # 6bd03f7
```

---

## F6（low）机器清单.md:12 "根分区 74% 用，25G 余" 实测 75%

```bash
grep -n "74% 用，25G 余" 状态/机器清单.md
timeout 30 ssh -o BatchMode=yes anolis 'df -h / | tail -1'
# /dev/vda3  98G  70G  25G  75% /
```
余量 25G 与文档一致，占用率已从 74% 漂到 75%（业务在跑，属正常漂移）。仅记录，不构成 DoD 违反（仍 <80%）。

---

## F7（low / 检查方法说明）30-遗产处置.md:63 "df 73%→67%" 与 3.1G 释放量在数量级上不自洽

`:61` 记录 `本机 ~/workspace/agent协作平面/（批1）… 3.1G … df 73%→67%`。本机 / 与 /home 同为 468G 文件系统（`/dev/nvme0n1p2 468G`）。468G 上 6 个百分点 ≈ 28G，与记录的 3.1G 差约 9 倍。当前实测 70%。

无法复现 73%→67% 这两个数（删除已完成，df 不可回溯）。两种可能：df 数字取自不同挂载点/不同时间窗口，或数字有误。**不判定为造假，判定为"该验证栏不可复现、自相矛盾"**。

```bash
df -h / | tail -1        # /dev/nvme0n1p2  468G  310G  134G  70% /
python3 -c "print('6%% of 468G =', round(468*0.06), 'G vs 记录 3.1G')"
```

---

## F8（low）30-遗产处置.md:63 救援落点标注不全

`:63` 写 temp2 中删除的副本为 `实施方案/设计v2/选型×2/外部研究/planning；设计快照先抢救入 参考/legacy/设计快照/`。实测 `参考/legacy/设计快照/` 只有 `Agent工厂设计_v2.md`、`选型挑战复审报告.md`、`选型终局建议_第三轮.md` 和 `planning/`；而 **实施方案与外部研究并不在该目录**，它们在 `设计/`（`设计/Agent工厂底座_实施方案_v1.md` + `设计/外部项目研究/` 9 份）。删除记录未注明这两个去了 `设计/`（同文件 :22 才有"设计文档已拷入 设计/"的说法）。内容未丢，仅记录不精确。

```bash
ls 参考/legacy/设计快照/            # 3 个 md + planning/
ls 设计/ 设计/外部项目研究/         # 实施方案_v1 + 9 份外部研究
ls ~/workspace/临时工作区/temp2/ | grep -E "实施方案|外部研究|选型"   # 已无
```

---

## 检查项 3 详情：af 提交 4dc1040 只含格式化 —— 证实

`4dc1040` 恰好 39 文件，**全部是 `crates/**/*.rs`**，无 Cargo.toml/lock/配置混入。

**本轮实际跑的两道检查：**

**(a) 任务书指定的"± 行数对称性抽查"—— 不成立，如实报告：**
```bash
git -C af show --numstat --format="" 4dc1040 | awk '{a+=$1;d+=$2}END{print "+"a" -"d}'
# +1389 -434
git -C af show --numstat --format="" 4dc1040 | grep dst_night_shift
# 195  64  crates/af-calendar/tests/dst_night_shift.rs
```
行数严重不对称。但对称性对 rustfmt 本就不是有效判据（折行会净增行数），故另跑了一道更强的检查。

**(b) 全部 39 文件 token 流比对（比抽样更强）—— 通过：**
```bash
cd ~/workspace/agent黑灯工厂/af && git show --numstat --format="" 4dc1040 | while read a d f; do
  git show 4dc1040^:"$f" | tr -d '[:space:]' > /tmp/old.txt
  git show 4dc1040:"$f"  | tr -d '[:space:]' > /tmp/new.txt
  cmp -s /tmp/old.txt /tmp/new.txt || echo "DIFFERS: $f"
done
```
25/39 文件连"去全部空白"都不等；再加一层去逗号（rustfmt 会增删尾逗号）后仅剩 6 个文件：
`af-calendar/tests/dst_night_shift.rs`、`af-credential/src/lib.rs`、`af-effect-gateway/src/lib.rs`、`af-meter/tests/routing.rs`、`af-release-gate/src/sim.rs`、`af-stop/src/lib.rs`。

逐个人工核了这 6 个的 diff，**全部是 rustfmt 的语义惰性变换**，未见任何逻辑改动：
- import 项重排（`crates/af-credential/src/lib.rs:21` `authorize, attenuate` → `attenuate, authorize`；`crates/af-meter/tests/routing.rs:5` `{self, SCHEDULE, DAY, WEEK}` → `{self, DAY, SCHEDULE, WEEK}`）
- 单表达式闭包加花括号（`dst_night_shift.rs` 内 `unwrap_or_else(|e| panic!(..))` → `unwrap_or_else(|e| { panic!(..) })`）——这是该文件 token 差异的唯一来源
- 结构体字面量/宏参数折行展开（`af-stop/src/lib.rs`、`af-effect-gateway/src/lib.rs`、`af-release-gate/src/sim.rs`）
- 一处注释缩进被 rustfmt 尾注释对齐拖到第 47 列（`crates/af-calendar/tests/dst_night_shift.rs:145`），纯观感问题

**结论**：4dc1040 只含格式化，声明属实。`cargo fmt --check --all` 实测绿。

---

## 检查项 4 详情：救援齐套性 —— 齐套 ✅

| 30-遗产处置.md:61-63 声明 | 实测 | 结果 |
|---|---|---|
| ADR 15 份 | `ls 参考/legacy/ADR/ADR-*.md \| wc -l` = 15 | ✅ |
| ADR INDEX | `参考/legacy/ADR/INDEX.md` 存在，索引列出 ADR-0001..0015 | ✅ |
| 任务1 盘点报告 3 份 | `参考/legacy/任务1-机器现状盘点/` 下 3 份（汇总-GLM-5.3 / 汇总-step-5-preview / 最终报告） | ✅ |
| 设计快照 | `参考/legacy/设计快照/` 3 md + `planning/taskset-redesign/`（93 文件） | ✅（落点标注不全见 F8） |
| `~/workspace/agent协作平面/` 已删 | `ls -d` → No such file or directory | ✅ |
| `~/workspace/anolis-mirror/` 已删 | `ls -d` → No such file or directory | ✅ |

```bash
find ~/workspace/agent黑灯工厂/参考/legacy -type f | wc -l   # 103
du -sh ~/workspace/agent黑灯工厂/参考/legacy/*               # ADR 76K / 任务1 192K / 设计快照 616K
ls -d ~/workspace/agent协作平面 ~/workspace/anolis-mirror
```

---

## 检查项 2 详情：组件建设状态.md ✅ 标记 vs 实际（实测通过项）

| 文档行 | 声明 | 复现命令 | 实测 |
|---|---|---|---|
| :25 | af/bin 五件套 ✅ 就位 | `ls af/bin; ./temporal --version` 等 | 5 个二进制齐全且可执行；temporal 1.9.1、zot v2.1.22(commit v2.1.22-0-g6bbc7e5, dist-spec 1.1.1)、cue v0.17.1、crane 0.22.1，与 `af/BOOTSTRAP.md:24-28` 逐条一致；cosign 输出 ASCII banner（BOOTSTRAP.md:14 记 v3.1.3）✅ |
| :30 | ClickHouse ✅ 双机就位 26.7.34.3，sha256 一致 a69c59fa… | `~/ch-mes/bin/clickhouse --version; sha256sum ~/ch-mes/bin/clickhouse` | 本机：`ClickHouse local version 26.7.34.3`；`a69c59fa73794d42f730739584d8b52b9297392a261f0ca5610b4672b0e82ffc` 前缀吻合 ✅。**anolis 侧 `sha256sum /home/anuser/ch-mes/bin/clickhouse` → Permission denied，"双机 sha256 一致"无法核实** |
| :31 | OTel ✅ `~/外部组件/otelcol/otelcol-bin`，ocb v0.163.0 四组件，components 验证过 | `~/外部组件/otelcol/otelcol-bin components` | otelcol-bin 存在（49.9MB）；components 输出 otlp / batch / debug / file 四个组件，均为 v0.163.0 ✅（注：`buildinfo.version` 为空串） |
| :32 | rekor-tiles ✅ 双变体 | `ls ~/外部组件/rekor-tiles/rekor-server*` | `rekor-server`(65.9MB) + `rekor-server-posix`(33.2MB) 均在 ✅ |
| :36 | grok 并发配方固化 5 实例 | `ssh anolis 'ls /data/af-factory'` | `i1..i5` 五个实例目录 + `tasks/` 下 T2/T3/T4/T56/T7 五张任务卡 + `logs/` 下 5 份实例日志 + `launch.sh` ✅ |
| :37 | gitea ✅ 双向通（anolis 往返探针 b5c6f1b） | `ssh anolis 'git -C /data/af-factory/af-sync log --oneline -2'`；`git -C af ls-remote origin` | anolis af-sync 有 `b5c6f1b roundtrip 探针：anolis 稀疏克隆→gitea push 通道验证`；gitea 上 `refs/heads/probe/anolis-roundtrip = b5c6f1b`，fetch remote 带 `[blob:none]`（稀疏）✅ |
| :26 | CI 六门 | `grep -n 'step[0-9]' af/ci/run.sh` | 六门齐全（run.sh:75,80,85,90,122,147）✅（5/6 vs fmt 已绿见 F3） |

**未跑**：`:5/:44/:16` 的"174+1 全绿 / 29+1 测试"本轮**未执行**测试套件，不作为通过项计入。

---

## 审查副作用声明（必须记录）

本轮为核对 `组件建设状态.md:44` 的"CI fmt 门转绿"，我在 af 目录执行了 `cargo fmt --check --all`。该仓库 `rust-toolchain.toml` 钉 `channel = "1.99.1"`，触发 rustup 对该 toolchain 做重同步；rustup 判定其"partially installed"，**先卸载了 cargo / rust-std / rust-docs 组件，随后下载失败并回滚不完整**，导致本机 1.99.1 toolchain 一度 `cargo` 与 `rust-std` 缺失。

**已修复并复验**：
```bash
rustup component remove cargo  --toolchain 1.99.1 && rustup component add cargo  --toolchain 1.99.1
rustup component remove rust-std --toolchain 1.99.1 && rustup component add rust-std --toolchain 1.99.1
rustup component list --toolchain 1.99.1 | grep installed
# cargo / clippy / rust-std / rustc / rustfmt 全部 (installed)
cargo +1.99.1 --version   # cargo 1.99.1 (5f94df478 2026-08-27)
rustc +1.99.1 --version   # rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo +1.99.1 fmt --version  # rustfmt 1.10.0-stable (58cf8f9c35 2026-10-13)
git -C af status --short | wc -l   # 0（工作区未被改动）
```
版本号与 `af/BOOTSTRAP.md:20-22` 记录一致，toolchain 已恢复原状，af 工作区无改动。

**暴露的隐患（建议主代理处理）**：本机 1.99.1 toolchain 处于 rustup 认为"部分安装"的状态，且 `~/.rustup/downloads` 在 15:38 前后出现过下载文件丢失。任何在该目录跑 `cargo` 的动作都可能触发同样的重装失败。本轮已修复，但根因（downloads 目录被并发清理/沙箱写入受限）未查明。
