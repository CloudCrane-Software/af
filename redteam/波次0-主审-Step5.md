# 波次 0（环境就绪）红队主审报告 · Step5

- **审查者**：红队主审 · Step5（未参与本项目建设；立场=找会出问题的点，不判是否合格）
- **审查时间**：2026-10-19 15:40–16:10 CST（repo HEAD 在审查期间从 5c745d2 漂到 751754c，文中均标注时点）
- **审查对象**：af 提交 `2e06908`（迁入）→ `4dc1040`（fmt 全量）→ `5c745d2`（T9a 工具链+deny），及其依赖的波次 0 文档（计划/10、状态/openBao.md §11、状态/机器清单.md、计划/30-遗产处置.md、计划/变更日志.md）与 af 仓库现状（含审查期间新落地的 T9b `6bd03f7` 及后续合流提交）。
- **方法**：只读审查。除本报告文件外未修改 af 仓库任何文件；未执行任何 git commit / push。所有结论附复现步骤。

## 0. 环境副作用声明（诚实披露，必读）

本轮审查动了以下本机状态，均与"只读审查 af 仓库"不冲突，但须记录：

1. 在 af 目录执行 rustup/cargo 命令，**触发 rustup 自动下载并安装 1.99.1 toolchain**（安装前本机未装该版本；输出 `info: syncing channel updates for 1.99.1-x86_64-unknown-linux-gnu`）。这正是发现 F5 的复现方式，也说明该 pin 对每台进仓库的机器都有强制副作用。
2. 为离线跑 advisories 检查，把 `af/ci/advisory-db`（11M）rsync 到 `~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`。
3. 在 `/home/pan-ding/rt-verify`、`/home/pan-ding/rt-fmt` 用 `git archive` 建立 5c745d2 / 2e06908 / 4dc1040 净树并跑了一次全量 cargo test（新建 2.8G target）；报告完成后已清理。
4. 第一次全量 cargo test 跑在**活的工作副本**上，因在途 rebase 冲突失败——该失败本身即 F7 的证据，非审查操作失误。
5. `/tmp` 为 tmpfs 且配额小（7.5G，解包前已用 3.9G）：一次 829M 解包即触到 `超出磁盘配额`，与 计划/10-环境与分工.md:19 的警告一致；后续解包改到 /home/pan-ding 下完成。

## 1. 结论摘要（按 severity 排序）

| # | severity | 一句话 |
|---|---|---|
| F1 | high | anolis 11.3G 他人活跃数据已不可逆删除：强制删除记录未登记、"已推远端"无法核实、删除前 2 小时该目录仍在被写 |
| F2 | high | 5 把私钥（无加密/空口令/口令写在仓库里）+ 3191 个 target 编译产物进 git，并已随 main 推到 gitea 私有镜像 |
| F3 | medium | 9 个 grok `--always-approve` 实例（文档称 5）以同一身份共享可读的 gitea 写令牌、空口令 SSH 私钥、refresh token |
| F4 | medium | deny.toml 两道门实跑即红（licenses 11 个 crate 无 license 字段；advisories RUSTSEC-2026-0173），且无任何 CI 门接线；T9b 绕行旗标在已装 cargo-deny 上不存在 |
| F5 | medium | rust-toolchain.toml 钉 4 天前发布的 1.99.1：每次进仓库触发 rustup 联网自装，已实际造成本机 cargo 组件被卸+下载失败回滚 |
| F6 | medium | gitea 镜像 main 停在 4dc1040（本地领先 9+ 提交）、gov 单机单容器、HTTP 明文、admin 凭据存于 smoke 域 |
| F7 | medium | 共享工作副本在途 rebase 冲突（UU af-workorder/Cargo.toml）使全 workspace cargo 失败——实例补丁与主代理并行改同一批文件 |
| F8 | medium | 下载清单"8/8 ✅"之一 Bifrost 源码实为损坏克隆（无工作树、HEAD=.invalid） |
| F9 | medium | anolis 工具链=本机 rsync 整体拷贝：无独立签名/校验锚点，本机缓存污染直达 9 实例执行面，deny sources 门不覆盖本地缓存 |
| F10 | low | hk-gateway 单出口 2 vCPU/1.9G（实测 load 2.85、gost 单进程），抖动即 9 实例同时掉线 |
| F11 | low | 文档漂移三连：实例 5→9、fmt 门"红/绿"同行矛盾、基线停在 60f6861 |
| F12 | low | fmt 全量提交本身已验证语义中立；残留流程风险：全量盲 fmt 无"只含格式"护栏、1389 机械行淹没逻辑史 |
| F13 | low | 杂项：README 指向仓外路径在镜像克隆中成死链；任务 prompt 经 argv 对 ps 可见；共享 target 锁风险；/tmp tmpfs 近满 |

---

## 2. 详细发现

### F1（high）anolis 11.3G 他人活跃数据删除：不可逆、未登记、备份声称不可核实

**位置**：`计划/30-遗产处置.md:33`（"不动，登记观察"）与 `:40`（DoD 挂起）↔ `状态/机器清单.md:36` 与 `计划/变更日志.md:8`（"运营者授权后删 /home/anuser/agentic-factory-projects（11.3G，已推远端）"）。

**问题**：三份文档互相矛盾。30 号文件（处置计划的唯一权威）白纸黑字记录该目录"**今日 14:16 仍在被写（另有活跃项目）**"，处置结论是"不动，登记观察"，DoD 项"挂起，不算完成"；而机器清单/变更日志记录它已被删。30 号文件 §3 的强制 append-only 删除记录表（`:59-63`）**只有 3 行且全部是本机条目，没有这一条 anolis 删除**——违反该文件自己第 4 行的纪律（"每批删除前在本文件'删除记录'表登记"）。"已推远端"推到哪里、完整性如何，文档无记载，且从我所在位置无法核实。

**证据**：

```
# 计划/30-遗产处置.md:33（盘点结论表内该行）
| /home/anuser/agentic-factory-projects | ≥3.4G（含隐藏项共 11.3G） | **今日 14:16 仍在被写**（另有活跃项目） | 不动，登记观察 |
# 计划/30-遗产处置.md:59-63（删除记录表仅 3 行，全部本机）
# 状态/机器清单.md:36
| 2026-10-19 | ZCode 主代理 | anolis 根分区 93%→**74%**：运营者授权后删 /home/anuser/agentic-factory-projects（11.3G，已推远端）……
```

```
$ timeout 30 ssh -o BatchMode=yes anolis 'df -h / | tail -1; ls -d /home/anuser/agentic-factory-projects; ls /home/anuser/'
/dev/vda3        98G   70G   25G  75% /
ls: cannot access '/home/anuser/agentic-factory-projects': Permission denied
ls: cannot open directory '/home/anuser/': Permission denied
```

**复现步骤**：
1. `grep -n "不动，登记观察\|现实不可达\|挂起" 计划/30-遗产处置.md`
2. `sed -n '57,64p' 计划/30-遗产处置.md`（删除记录表无 anolis 行）
3. `grep -n "已推远端" 状态/机器清单.md 计划/变更日志.md`
4. `timeout 30 ssh -o BatchMode=yes anolis 'df -h / | tail -1; ls -d /home/anuser/agentic-factory-projects; sudo -n true'`（Permission denied / sudo 需密码 → 删除与备份声称均不可核实）

**会出什么问题**：若"已推远端"不实或不完整（删除时刻距"仍在被写"仅约 2 小时，非静止快照），anuser 的活跃项目数据永久丢失且无恢复点；文档层面，后续任何人按 30 号文件做清理决策都会按"未删"进行，按机器清单做恢复决策又找不到备份位置。运营者授权存在（机器清单:36），但授权不能替代登记与备份核验。

---

### F2（high）5 把私钥 + 3191 个编译产物进 git，并已同步到 gitea 私有镜像

**位置**：af 仓库 git 历史（PoC 种子提交 `8b07a7d` / `60f6861` 引入，`4dc1040` 已作为 main 推到 gitea 镜像）。

**问题**：
1. **私钥入库**：`poc/poc-rekor/keys/log-ed25519-priv.pem` 是**无加密** PKCS#8 ed25519 私钥（`-----BEGIN PRIVATE KEY-----`，正文即密钥材料）；4 把 cosign 密钥（`poc/poc-cas/verify-tr/tr-key.key`、`poc/poc-cas/cosign.key`、`poc/poc-cas-rust/casr-sign.key`、`poc/poc-rekor/keys/cosign-gen/trace-signer.key`）虽为 `ENCRYPTED SIGSTORE PRIVATE KEY` 头，但 `casr-sign.key` 的生成口令是**空串**（`poc/poc-cas-rust/REPORT.md:82` `COSIGN_PASSWORD=""`），`trace-signer.key` 的口令 `poc-test-pass` **明文写在仓库 REPORT.md 里**（`:35`、`:73`、`:162`）——加密形同虚设。
2. **CI 验签门（ci/run.sh step6）的证据价值失效**：gate6 用 `tr-key.pub + tr-bundle.json + trusted-root.json` 验签，而签名私钥 `tr-key.key` 就躺在同一仓库——任何有镜像读权限的人都可以伪造能过 gate6 的工件。
3. **编译产物入库**：4243 个跟踪文件中 **3191 个（75%）在 `**/target/` 下**，其中 519 个 `.rlib/.rmeta/.so` 二进制；`.git` 已达 **1009M**；全量树 5.2G（`状态/openBao.md:378`）迫使 anolis 用 blob:none 稀疏克隆——**稀疏克隆没有 blob，不构成备份**。不可审查的旧工具链构建产物进 VCS，既是供应链面也是仓库面。

**证据**：

```
$ git ls-files | wc -l            # 4253（审查期间 HEAD 漂移，5c745d2 时为 4243）
$ git ls-files | grep -c '/target/'
3191
$ git ls-files | grep -cE '\.(rlib|rmeta|so)$'
519
$ du -sh .git
1009M	.git
$ git ls-files | grep -iE '\.(key|pem)$'
poc/poc-cas-rust/casr-sign.key
poc/poc-cas/cosign.key
poc/poc-cas/verify-tr/tr-key.key
poc/poc-rekor/keys/cosign-gen/trace-signer.key
poc/poc-rekor/keys/log-ed25519-priv.pem
poc/poc-rekor/keys/log-ed25519-pub.pem
poc/poc-sandbox/cube-rootCA.pem
$ head -1 poc/poc-rekor/keys/log-ed25519-priv.pem
-----BEGIN PRIVATE KEY-----
$ grep -n 'COSIGN_PASSWORD=""' poc/poc-cas-rust/REPORT.md
82:$ COSIGN_PASSWORD="" cosign generate-key-pair --output-key-prefix casr-sign
$ grep -n 'poc-test-pass' poc/poc-rekor/REPORT.md | head -3
35:…（ECDSA P-256，`COSIGN_PASSWORD=poc-test-pass`）
73:$ COSIGN_PASSWORD=poc-test-pass cosign generate-key-pair --output-key-prefix trace-signer
162:cosign 加密密钥当 log signer：--signer-filepath trace-signer.key --signer-password=poc-test-pass
$ git merge-base --is-ancestor 8b07a7d 4dc1040 && echo "gitea main(4dc1040) 含 PoC 种子提交"
gitea main(4dc1040) 含 PoC 种子提交
```

**复现步骤**：
1. `cd af && git ls-files | grep -c '/target/' && git ls-files | grep -cE '\.(rlib|rmeta|so)$' && du -sh .git`
2. `git ls-files | grep -iE '\.(key|pem)$'` 逐把 `head -1`
3. `grep -n 'COSIGN_PASSWORD' poc/poc-cas-rust/REPORT.md poc/poc-rekor/REPORT.md | head`
4. `git ls-remote origin`（main=4dc1040）→ `git merge-base --is-ancestor 8b07a7d 4dc1040`（密钥已在镜像中）

**会出什么问题**：这些密钥目前按各 REPORT 是 PoC 一次性资产，但仓库同时是未来 release-gate/cosign 正式签名能力的来源——一旦有人图方便复用（密钥就在手边），泄漏立即生效且已无法从 git 历史与镜像中无声去除（需 history rewrite + 镜像 purge + 全部克隆作废）。缓解事实也如实记录：`.gitignore` 的 `**/target/` 规则在 `60f6861` 已加入，只是对已跟踪文件无效；poc-dspy 的 .venv 仅在工作区、未被跟踪。

---

### F3（medium）grok `--always-approve` × 9 实例 × 共享凭据面（文档称 5）

**位置**：`计划/10-环境与分工.md:26,33,36`（配方）↔ anolis `/data/af-factory` 实时状态（15:55 实测）。

**问题**：
1. **规模已超文档一倍**：文档与变更日志都写"5 实例稳定并发 / 五实例"，实测 **i1..i9 共 9 个实例目录、17 个 grok 进程、9 张任务卡**（i6–i9 为 15:40 启动的 round2）。全部以 `--always-approve --max-turns 100 --no-plan` 运行——所有工具调用（文件写、shell 执行）无人工闸门，单实例最多 100 轮自主动作。
2. **凭据与实例同身份同盘**：实例以 `pan-ding` 运行，而 `/data/af-factory/.git-token`（gitea `write:repository` 令牌，0600 pan-ding）、`~/.ssh/opch_gitea_ed25519`（**实测空口令**：`ssh-keygen -y -P ""` 可直接导出公钥）、`~/.grok/auth.json`（OIDC refresh token，0600）对每个实例可读。实例 origin 虽指向本地 `af-work`（不能直达 gitea），但 auto-approve 下实例可自行配置 remote/askpass 外推；凭据一旦被 `cat`，内容同时进入 TUI 转录日志（`/data/af-factory/logs/*.log`）与发往 x.ai 的模型上下文（第三方）。
3. **任务全文经 argv 传递**：`launch.sh` 用 `grok ... "$(cat $PROMPT)"`，`ps aux` 对任何本机用户可见任务全文。
4. **额度今日 17:20 过期**（`计划/10:40`）：9 个实例可能同时中途死亡，留下半成品工作树；共享 `CARGO_TARGET_DIR`（2.8G）下被 kill 的 cargo 可能留锁，卡死其余实例的构建。

**证据**：

```
$ timeout 60 ssh -o BatchMode=yes anolis 'ls -d /data/af-factory/i* | wc -l; ps aux | grep -c "[g]rok"'
9
17
$ ssh anolis 'ls -la /data/af-factory/.git-token ~/.grok/auth.json; ls -la ~/.ssh/'
-rw------- 1 pan-ding pan-ding   41 Oct 19 15:02 /data/af-factory/.git-token
-rw------- 1 pan-ding pan-ding 1757 Oct 19 12:46 /home/pan-ding/.grok/auth.json
-rw------- 1 pan-ding pan-ding  411 Oct  8 22:09 opch_gitea_ed25519        ← 私钥 411B
-rw-r--r-- 1 pan-ding pan-ding  100 Oct  8 22:09 opch_gitea_ed25519.pub    ← 对应公钥
$ ssh anolis 'ssh-keygen -y -P "" -f ~/.ssh/opch_gitea_ed25519 >/dev/null 2>&1 && echo EMPTY-PASSPHRASE || echo NEEDS-PASSPHRASE'
EMPTY-PASSPHRASE(可无口令直接使用)
$ ssh anolis 'ps aux | grep "[g]rok" | head -2 | cut -c1-160'
pan-ding  329196  ... grok --always-approve --max-turns 100 --no-plan ?????? af???Agent ?...
$ cat /data/af-factory/launch.sh   # 关键行
setsid script -qec "bash /tmp/$NAME.cmd" "$LOG" >/dev/null 2>&1 &
# 其中 /tmp/$NAME.cmd 内：grok --always-approve --max-turns 100 --no-plan "\$(cat $PROMPT)"
$ cat /data/af-factory/env.sh
export RUSTUP_HOME=/data/af-factory/.rustup
export CARGO_HOME=/data/af-factory/.cargo
export CARGO_TARGET_DIR=/data/af-factory/target
export HTTPS_PROXY=socks5h://100.64.0.3:1080 ALL_PROXY=socks5h://100.64.0.3:1080
$ grep -rlE "git-token|token.*=.*[A-Za-z0-9]{30,}" /data/af-factory/logs/    # 日志泄密扫描
（无输出——目前未发现泄漏，属运气/纪律，非机制保证）
```

**复现步骤**：
1. `timeout 30 ssh -o BatchMode=yes anolis 'ls -d /data/af-factory/i* | wc -l; ps aux | grep -c "[g]rok"; ps aux | grep "[g]rok" | head -1 | cut -c1-160'`
2. `ssh anolis 'ls -la /data/af-factory/.git-token ~/.grok/auth.json ~/.ssh/; ssh-keygen -y -P "" -f ~/.ssh/opch_gitea_ed25519 >/dev/null 2>&1 && echo EMPTY-PASSPHRASE || echo NEEDS-PASSPHRASE'`
3. `ssh anolis 'cat /data/af-factory/launch.sh /data/af-factory/env.sh'`

**会出什么问题**：单个实例被任务卡里的恶意/幻觉内容诱导（或被任何能写 `/data/af-factory/tasks/`、`patches/` 的人投毒——任务卡目录无隔离，9 实例互可读写），即可外泄 gitea 写令牌或 SSH 私钥，或把带毒补丁推进 `patches/` 等主代理合流。主代理是唯一人工闸，且在额度 17:20 到点的时限压力下审查补丁。任务卡纪律头（common-header.txt：先读 RESULT.md、限定改动范围、fmt/clippy -D warnings/test 阶梯）写得很好，但执行者是自报告全绿的 grok 本身。

---

### F4（medium）deny.toml 两道门实跑即红，无 CI 接线；T9b 绕行旗标在已装版本不存在

**位置**：`af/deny.toml`（5c745d2 引入）、`af/ci/run.sh`（六门不含 deny）、`af/ci/refresh-advisory-db.sh`（6bd03f7 T9b）。

**问题**：commit message 自认"首发最小集，跑 cargo deny check 后按输出补全"——即**没跑过就提交**。我在 5c745d2 净树用钉住的 1.99.1 实跑四类检查：

```
$ cargo deny check bans sources licenses
…
error[unlicensed]: af-calendar = 0.1.0 is unlicensed
error[unlicensed]: af-credential = 0.1.0 is unlicensed
… （11 个 workspace crate 全部中招：Cargo.toml 无 license 字段）
warning[duplicate]: found 2 duplicate entries for crate 'thiserror'（1.0.69 + 2.0.21）
warning[duplicate]: found 2 duplicate entries for crate 'unicode-width'（0.1.14 + 0.2.2）
bans ok, licenses FAILED, sources ok
DENY_EXIT=4
```

```
$ cargo deny --offline check advisories        # 需先把 ci/advisory-db 预置进 ~/.cargo/advisory-dbs
error[unmaintained]: proc-macro-error2 is unmaintained
 ├ ID: RUSTSEC-2026-0173（Solution: No safe upgrade is available!）
 ├ proc-macro-error2 v2.0.1 ← biscuit-quote v0.3.0 ← biscuit-auth v6.0.0 ← af-credential v0.1.0
advisories FAILED
```

```
$ cargo deny check advisories                 # 不预置 db 的默认路径（T9b 想绕开的场景）
[ERROR] failed to fetch advisory database https://github.com/RustSec/advisory-db …
fatal: 无法访问 'https://github.com/RustSec/advisory-db/'：GnuTLS recv error (-110)
$ cargo deny check advisories --db-path …     # T9b commit message 写的用法
error: unexpected argument '--db-path' found   # cargo-deny 0.20.2 不存在该旗标
```

**问题归纳**：(a) licenses 门红——11 个自家 crate 无 `license` 字段（根 Cargo.toml 已有 `[workspace.package]`，加 `license` + 各 crate `license.workspace = true` 即可修，但没人修）；(b) advisories 门红——`af-credential → biscuit-auth 6.0.0` 传递依赖 RUSTSEC-2026-0173（unmaintained，无安全升级），deny.toml 无 ignore；(c) ci/run.sh 六门**根本没有 deny**——配置是装饰品；(d) T9b 的绕行方案（`--db-path/--db --no-fetch`）在已装 cargo-deny 0.20.2 上不存在，唯一可行路径是 `cargo deny --offline check advisories` + 预置 db，而刷新脚本只做 `git -C advisory-db pull`，不做预置；(e) 本地 db 最后提交 2026-10-17 17:23，审查时已 2 天，刷新全靠手动。

**复现步骤**：
1. `git archive 5c745d2 crates Cargo.toml Cargo.lock rust-toolchain.toml deny.toml | tar -x -C <净树>`（绕开在途合并，见 F7）
2. `cd <净树> && cargo deny check bans sources licenses; echo EXIT=$?`
3. `rsync -a af/ci/advisory-db/ ~/.cargo/advisory-dbs/advisory-db-3157b0e258782691/ && cargo deny --offline check advisories; echo EXIT=$?`
4. `cargo deny check advisories`（默认 fetch 路径，复现 GitHub 失败）与 `cargo deny check advisories --db-path X`（复现旗标不存在）
5. `git -C af/ci/advisory-db log -1 --format='%cd'`（db 新鲜度）

**会出什么问题**：T9"cargo 阶梯接入"若按计划把 deny 接进 CI，第一天全红；不接则四类检查形同虚设，已知 unmaintained 传递依赖（biscuit 链）长期无人处置——而 af-credential 正是资格/凭证这种安全相关 crate。

---

### F5（medium）rust-toolchain.toml 钉 1.99.1：每次进仓库触发联网自装，已实际弄坏过本机 cargo

**位置**：`af/rust-toolchain.toml`（5c745d2）：`channel = "1.99.1"`、`components = ["rustfmt", "clippy"]`。

**问题**：1.99.1 发布于 **2026-10-15**（`rustup` 信息：`latest update on 2026-10-15 for version 1.99.1`），距提交仅 4 天，无 soak。`rust-toolchain.toml` 的存在使**任何**在仓库内的 cargo/rustc 调用都触发 rustup 自动管理该工具链：
- 本轮我自己在 af 目录跑 `rustup toolchain list` 即触发 `info: syncing channel updates for 1.99.1-x86_64-unknown-linux-gnu`（安装前本机未装）；
- `rustup which cargo` 一度返回 `error: 'cargo' is not installed for the toolchain '1.99.1-x86_64-unknown-linux-gnu'`；
- 更重的事故记录在交叉审文件 `af/redteam/波次0-交叉审-MiniMax.md:213-228`：rustup 判定 1.99.1 "partially installed" → **先卸载 cargo/rust-std/rust-docs → 下载失败 → 回滚不完整**，导致本机 cargo 缺失（15:38–15:55 经手动 `component remove/add` 修复）。
- ci/run.sh step0 预检跑 `rustup component add rustfmt clippy`——CI 预检本身依赖网络可达 static.rust-lang.org；而本项目自己的记录显示 GitHub 经代理克隆失败（5c745d2 commit message、T9b commit message），网络并不总是可达。

**复现步骤**：
1. `cd af && rustup toolchain list`（观察 syncing/自动安装行为；本机执行前未装 1.99.1）
2. `cd af && rustup which cargo`（在工具链未就绪窗口内复现 "cargo is not installed"）
3. `rustup run 1.99.1 rustc --version`（确认版本真实性：`rustc 1.99.1 (58cf8f9c3 2026-10-13)`——版本真实存在，问题在分发机制而非版本号造假）
4. 读 `af/redteam/波次0-交叉审-MiniMax.md:213-228`（部分安装→卸载→回滚失败事故记录）

**会出什么问题**：每台新机器/CI runner/新克隆首次进仓库都要联网装 1.99.1；网络抖动就得到一台"cargo 损坏"的机器（已发生一次，见交叉审）；钉 4 天前的编译器还意味着任何 rustc 回归立即全员命中。工具链在 anolis 侧靠 rsync 拷贝规避了这个问题（见 F9），但本机与 CI 没有这层规避。

---

### F6（medium）gitea 镜像陈旧 + 单点：main 停在 4dc1040，唯一 off-box 副本在 gov 单机

**位置**：`状态/openBao.md:371-378`（§11 gitea 通道）+ `git ls-remote origin`（15:50 实测）。

**问题**：
1. **镜像不是当前备份**：`git ls-remote origin` 显示 `refs/heads/main = 4dc1040`（fmt 提交），而本地 HEAD 审查期间已到 `751754c`（T5/T6 合流）——**本地领先 9+ 提交**（5c745d2、6bd03f7、873567b、ce475f4、5057b36、044e04a、c125885、e24bc9f、42201dc、751754c）。grok 实例的分支（task-T2/T3/T4/T56/T7）也完全不在 gitea 上（只有 main + probe 两支）。想从镜像恢复 = 回到 fmt 提交。
2. **单点**：gitea 是 gov（100.64.0.6）单机上的单容器 `opch-gitea`，与 openBao、pg、minio、temporal-ui 等 15 容器同宿主（宿主 up 23d、load ~3）。af 的完整副本只有两处：pandev-comp 工作副本 + gov gitea；anolis 侧是 `--filter=blob:none` 稀疏克隆（无 blob，不能当备份）。
3. **暴露面**：端点 `http://100.64.0.6:3300` 是**明文 HTTP**（tailnet WireGuard 缓解线上窃听，但 token/basic-auth 对 gov 宿主与链路上任何终端可见；若日后 Caddy 前置或 tailnet 被渗透即裸奔）；admin 凭据（admin_user/admin_pass/api_token）存在 `kv/company/agent-gitea-smoke/m0-round1`——一个名字像"冒烟测试"的密钥域（openBao.md:375 自述"smoke"），却承载生产镜像的管理员口令；anolis 的 scoped 令牌（write:repository）落在 9 个 auto-approve 实例可读的共享目录（F3）。

**证据**：

```
$ GIT_TERMINAL_PROMPT=0 git ls-remote origin
4dc10403570e96d4ff65ce168fb29c648f0e0b2d	HEAD
4dc10403570e96d4ff65ce168fb29c648f0e0b2d	refs/heads/main
b5c6f1b225c11f597b473107b7d4c99b6b2a98bc	refs/heads/probe/anolis-roundtrip
$ git log --oneline -1     # 15:55 时点
751754c af-workorder: T5 payload_hash→SHA-256 + T6 ed25519 变更单签名（grok i5，合流经三方合并）
$ git remote -v
origin	http://100.64.0.6:3300/opchadmin/af.git (fetch/push)     # 明文 HTTP；URL 内无凭据（askpass 设计 OK）
```

**复现步骤**：
1. `cd af && GIT_TERMINAL_PROMPT=0 git ls-remote origin && git log --oneline -1`（对比镜像 main 与本地 HEAD）
2. `grep -n "3300\|agent-gitea-smoke\|稀疏" 状态/openBao.md | head`
3. `grep -n "opch-gitea" 状态/机器清单.md`（gov 15 容器清单）

**会出什么问题**：gov 宕机/磁盘损毁 → 镜像与信任层（bao 同宿主）同时没了，只剩 pandev-comp 一份（同一 tailnet/同一故障域）；镜像陈旧 → 事后恢复停在 fmt 提交；smoke 域凭据治理强度不明 → admin 口令泄漏面。

---

### F7（medium）共享工作副本在途 rebase 冲突，已实际弄坏全 workspace cargo

**位置**：af 工作副本实时状态（15:45–15:52，分支 `wave1-merge`）。

**问题**：我 15:45 启动的全量 `cargo test --workspace` 直接死在 manifest 加载：

```
error: key with no value, expected `=`
error: failed to load manifest for workspace member `/home/pan-ding/workspace/agent黑灯工厂/af/crates/af-workorder`
```

同时 `git status` 显示 `UU crates/af-workorder/Cargo.toml`（文件里有 `<<<<<<< HEAD` 冲突标记：主代理侧 `serde = { workspace = true }` vs 实例侧 `serde = { version = "1", features = ["derive"] }`），`.git/` 下有 `rebase-apply`、`AUTO_MERGE`。根因清楚：grok 实例 i3 的 T2 任务（"各 crate 的公共依赖改为 workspace = true"，见 `patches/i3-T2.patch` 与 i3 提交 `056cb33`）与主代理并行修改同一批 Cargo.toml。15:52 复查冲突已解决、HEAD 变为 `751754c`（commit message 自述"合流经三方合并"）、af-graph 又有未暂存改动——窗口是分钟级，但期间任何 agent/CI 在该副本上跑门都会得到与自身改动无关的红。ci/run.sh step0 的注释自己都预判了这一点："workspace manifest 完整性（共享仓库里并发改动可能弄坏 members）"。

**证据**（我后台跑的活副本测试日志 + 当时 git status）：

```
$ cargo test --workspace 2>&1 | tail -40        # 15:45 启动
error: key with no value, expected `=`
error: failed to load manifest for workspace member `.../af/crates/af-workorder`
$ git status --short | head
M  Cargo.lock
 M crates/af-graph/Cargo.toml
 M crates/af-graph/src/validate.rs
UU crates/af-workorder/Cargo.toml
A  crates/af-workorder/tests/calendar_sla.rs
$ sed -n '10,30p' crates/af-workorder/Cargo.toml
<<<<<<< HEAD
serde = { workspace = true }
…
=======
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
$ git status | head -2
位于分支 wave1-merge
$ ls .git/ | grep -iE 'merge|rebase|cherry'
AUTO_MERGE
rebase-apply
```

**复现步骤**：该状态是时点性的，15:52 后已自愈。可复现的是**模式**而非状态：`patches/i3-T2.patch`（anolis）与主代理同时改 crates/*/Cargo.toml；审查期间 HEAD 从 5c745d2 → 751754c（10 个提交/约 70 分钟）且多次出现未暂存/冲突状态。判据：在合流窗口内 `cd af && cargo metadata --no-deps` 必失败。

**会出什么问题**：门禁红绿不可信（可能是别人的在途编辑）；按任务卡跑的 grok 实例若共用该副本会莫名失败（目前实例用本地 af-work 克隆，暂未直连——但主代理在本副本上合流，窗口期一切自动化都不可靠）；`git log` 里"三方合并"提交与实例补丁双轨合流，出问题时难定位是哪一路引入。

---

### F8（medium）下载清单"8/8 ✅"之一 Bifrost 源码实为损坏克隆

**位置**：`计划/变更日志.md:8` 第 ④ 项 ↔ 本机 `~/外部组件/bifrost/`（15:55 实测）。

**问题**：变更日志声称"④下载清单 8/8 ✅（Bifrost 源码/…）"，实际 Bifrost 目录只有一个 `.git`，**无工作树、无任何提交**，HEAD 指向 `refs/heads/.invalid`，残留 `.git/shallow.lock`——git 明确报分支损坏。而 §4-1 对 Bifrost 的要求是"无官方 release 二进制，钉 commit；文档超前二进制，以源码为准"——现在源码根本没拿到，后续 Bifrost 的 Go 构建没有前置。

**证据**：

```
$ ls -a ~/外部组件/bifrost/
.  ..  .git
$ cat ~/外部组件/bifrost/.git/HEAD
ref: refs/heads/.invalid
$ ls ~/外部组件/bifrost/.git/*.lock
/home/pan-ding/外部组件/bifrost/.git/shallow.lock
```

**复现步骤**：`ls -a ~/外部组件/bifrost/ && cat ~/外部组件/bifrost/.git/HEAD && ls ~/外部组件/bifrost/.git/*.lock`（与交叉审 `af/redteam/波次0-交叉审-MiniMax.md:19-36` F1 独立互证）。

**会出什么问题**：按 8/8 ✅ 口径排期/交付 → 用到时发现要从头重克隆（且要再走一次拥塞的 GitHub/hk 代理）；append-only 变更日志的强口径已被污染，后续"清单打勾"的可信度整体打折。

---

### F9（medium）跨机 rsync 工具链：无签名锚点的供应链，本机缓存污染直达执行面

**位置**：`计划/10-环境与分工.md:37`（"用本机 ~/.rustup+~/.cargo 整体 rsync 到 /data/af-factory/.rustup/.cargo，版本 1.99.1 与本机完全对齐"）+ anolis `/data/af-factory/env.sh`。

**问题**：anolis 无 rust，9 个实例 + 2.8G 共享 target 的全部构建信任一份**从 pandev-comp rsync 来的二进制拷贝**（rustc/cargo/std + 1.7G registry 缓存）。rsync 只保证传输一致性，不保证来源可信：pandev-comp 的 `~/.cargo/registry` 缓存若被污染（typosquat/篡改进缓存），污染随下一次 rsync 直达执行面；9 个实例离线构建（无 credentials.toml、无 config.toml，aniis 侧实测）意味着**没有任何一次构建会联网校验**。`deny.toml` 的 `[sources] unknown-registry = "deny"` 只查 manifest 声明来源，不查本地缓存内容。另：anolis 实测 `rustc 1.99.1` 与本机一致（对齐本身做到了），但"对齐"锚点只有版本号，没有官方分发校验（如 rustup manifest 签名之外的独立核验）。

**证据**：

```
$ cat 计划/10-环境与分工.md:37
- 工具链：anolis 无 rust——用本机 ~/.rustup+~/.cargo 整体 rsync 到 /data/af-factory/.rustup/.cargo（RUSTUP_HOME/CARGO_HOME 指过去，版本 1.99.1 与本机完全对齐）。
$ ssh anolis 'cat /data/af-factory/env.sh'
export RUSTUP_HOME=/data/af-factory/.rustup
export CARGO_HOME=/data/af-factory/.cargo
export CARGO_TARGET_DIR=/data/af-factory/target
$ ssh anolis 'ls -la /data/af-factory/.cargo/ | head; RUSTUP_HOME=/data/af-factory/.rustup /data/af-factory/.cargo/bin/rustc --version'
（无 credentials.toml、无 config.toml）
rustc 1.99.1 (58cf8f9c3 2026-10-13)
$ ssh anolis 'du -sh /data/af-factory/target/; df -h /data | tail -1'
2.8G	/data/af-factory/target/
/dev/vdb1       295G   67G  213G  24% /data
```

**复现步骤**：`ssh anolis 'cat /data/af-factory/env.sh; ls -la /data/af-factory/.cargo/; RUSTUP_HOME=/data/af-factory/.rustup /data/af-factory/.cargo/bin/rustc --version'`。

**会出什么问题**：单点污染全链——pandev-comp 的缓存/工具链被动手脚一次，9 个 auto-approve 实例全部构建带毒产物，且离线构建无从发现；反向也没有"拉取+校验"的独立通道。（缓解事实：本机与 anolis 均无 cargo credentials.toml，无 registry 令牌可窃；rsync 本身有校验和。）

---

### F10（low）hk-gateway 单出口：小盒子扛全量外网

**位置**：`计划/10-环境与分工.md:12,16` + `状态/机器清单.md:13` + 100.64.0.3 实测。

**问题**：2 vCPU / 1.9G 内存 / 29G 盘的单机，是 anolis **一切**外网的唯一出口（SOCKS5 1080 + exit node）：9 个 grok 实例的模型 API、OIDC 续期（续期失败=实例集体掉线）、cargo 的 GitHub 访问、advisory-db 刷新全过它。实测 load 2.85、内存 901M/1967M、磁盘 30%、gost active。30 号文件批 4 还记着 gost.log 354M 待截断。它同时是 tailscale exit node——是整平面的单点。

**证据**：

```
$ timeout 20 ssh -o BatchMode=yes root@100.64.0.3 'uptime; free -m | head -2; df -h / | tail -1; systemctl is-active gost'
 07:48:39 up 14 days,  4:03,  2 users,  load average: 2.85, 2.81, 2.62
               total        used        free      shared  buff/cache   available
Mem:           1967         901         193          1066        1066        1066
/dev/vda1        29G  8.5G   21G  30% /
active
```

**复现步骤**：`ssh root@100.64.0.3 'uptime; free -m | head -2; systemctl is-active gost'`（本机持有直连密钥，只读命令）。

**会出什么问题**：它抖动/重启 → 9 实例同时掉线 + auth 续期失败 + 半途任务；17:20 额度到期与它的可用性叠加，今天的波次 1 产出有整体中断风险。（gov 单机同理性，见 F6，不重复计。）

---

### F11（low）文档漂移三连

**位置**：`状态/机器清单.md:22`、`状态/组件建设状态.md:26 vs :44`、`:5/:62`。

**问题**：(a) 机器清单:22 与变更日志:10 都写"i1~i5 五实例"，实测 i1..i9（F3）；(b) 组件建设状态.md:26 写"CI 六门 5/6 绿，fmt 门红=预存 235 块"，同文件 :44 写"T1 fmt … CI fmt 门转绿"——同行矛盾，:26 是陈旧残留（fmt 门客观已绿，我复跑 `cargo fmt --check --all` EXIT=0 证实）；(c) 该文件 :5/:62 的 git 基线停在 60f6861，实际 HEAD 已 751754c。交叉审 MiniMax 已记录同类问题（其 F3/F5），我复核属实。

**复现步骤**：`grep -n "i1~i5\|五实例" 状态/机器清单.md 计划/变更日志.md`；`grep -n "fmt 门红\|fmt 门转绿" 状态/组件建设状态.md`；`grep -n 60f6861 状态/组件建设状态.md && git -C af rev-parse --short HEAD`。

**会出什么问题**：接手人按文档排容量（以为是 5 实例）、判断门禁状态（以为 fmt 还红）、定位基线（以为在 60f6861）——全错。低危但持续消耗信任。

---

### F12（low）fmt 全量提交：语义中立已证实，残留流程风险

**位置**：af 提交 `4dc1040`（39 文件，+1389/-434）。

**问题（先摆证实的部分）**：提交声明"仅格式变化，零语义改动 / 174 passed / 0 failed / 1 ignored 与基线一致"——**我实测证实**：
1. **逐字节复现**：`git archive 2e06908`（fmt 前）→ 在其上用**钉住的 1.99.1 rustfmt** 跑 `cargo fmt --all` → 与 `4dc1040` 整树 `diff -r` **0 行差异**（rustfmt 语义保序，故"零语义改动"成立，且证明钉 1.99.1 后 fmt 门不会因 rustfmt 版本漂移翻红）；
2. **测试基线复现**：5c745d2 净树（含 fmt + 工具链 pin + deny.toml）全量 `cargo test --workspace`：**174 passed / 0 failed / 1 ignored，EXIT=0**——与提交声明逐字一致；
3. `cargo fmt --check --all` EXIT=0。

**残留风险（本条 finding 的本体）**：(a) **全 workspace 盲跑 `cargo fmt` 没有"只含格式"的护栏**——这次我用逐字节复现证明了没混入逻辑改动，但流程上不保证下次（一个 staged 的真实改动会被无声扫进"fmt 提交"）；(b) 1389+/434- 的机械行与逻辑变更同史，后续 `git log -S`/blame 考古成本上升；(c) 提交同时是"CI 六门 fmt 门转绿"的依据——门确实绿了，但绿的原因是这次没混入东西，不是机制保证。

**复现步骤**：
1. `git archive 2e06908 crates Cargo.toml Cargo.lock | tar -x -C pre/ && git archive 4dc1040 crates Cargo.toml Cargo.lock | tar -x -C post/`
2. `cd pre && cargo fmt --all && diff -r --exclude=target ../pre ../post && echo IDENTICAL`（预期 0 行）
3. `git archive 5c745d2 … | tar -x -C v/ && cd v && cargo test --workspace 2>&1 | grep 'test result' | awk '{p+=$4;f+=$6;i+=$8}END{print "passed="p" failed="f" ignored="i}'`（预期 174/0/1）
4. `cd v && cargo fmt --check --all; echo EXIT=$?`（预期 0）

---

### F13（low）杂项

1. **README 指向仓外路径**：`2e06908` 给 README 加了 `../AGENT.md`、`../产品设计_拍板版.md` 等相对仓外引用——在 gitea 镜像/克隆里全是死链，克隆者无法自包含理解入口契约（af/README.md:3-5）。
2. **任务 prompt 经 argv 对 ps 可见**（F3 已引），且 `/tmp/$NAME.cmd` 以默认 umask 落盘（本机任何用户可读/可改——实例间无隔离）。
3. **共享 CARGO_TARGET_DIR 锁风险**：9 实例共用一个 2.8G target；17:20 额度到期若把实例连同其 cargo 一起 kill，残留的 package-cache 锁可能让其余实例全部 `Blocking waiting for file lock`。
4. **/tmp tmpfs 7.5G 已用 3.9G**（我一次 829M 解包即触配额）：计划 §2:19 早有"大文件放 /home/pan-ding"的纪律，但 tmpfs 仍在被悄悄填满（谁在填未查，超出只读范围）。

---

## 3. 已验证为干净的项目（负面结果，同样可复现）

| 检查 | 命令 | 结果 |
|---|---|---|
| fmt 提交语义中立 | `git archive 2e06908 → cargo fmt(1.99.1) → diff -r vs 4dc1040` | **0 行差异**（见 F12） |
| 测试基线 174/0/1 | 5c745d2 净树 `cargo test --workspace` | **174 passed / 0 failed / 1 ignored，EXIT=0** |
| fmt 门 | 净树 `cargo fmt --check --all` | **EXIT=0** |
| deny bans/sources | 净树 `cargo deny check bans sources` | **ok / ok**（仅 duplicate 警告：thiserror 1.0.69+2.0.21、unicode-width 双版——根 Cargo.toml 注释说明 thiserror 双主版本是刻意决定） |
| git 依赖 / 通配版本 | `grep -rn 'git *=\|version *= *"\*"' crates/*/Cargo.toml Cargo.toml` | 均无 |
| cargo registry 凭据 | `ls ~/.cargo/credentials.toml`（本机）；`ssh anolis 'ls /data/af-factory/.cargo/credentials.toml'` | 两台均**不存在**（rsync 不会带走 registry 令牌） |
| grok 日志泄密 | `ssh anolis 'grep -rlE "git-token\|token.*=.*[A-Za-z0-9]{30,}" /data/af-factory/logs/'` | 无命中（当前未泄漏） |
| anolis 工具链对齐 | `ssh anolis 'RUSTUP_HOME=… rustc --version'` | 1.99.1 与本机一致 |
| gitea 凭据不入 URL | `git remote -v` | URL 无凭据；askpass 管道取 token（openBao.md:376 设计） |
| gitea 往返探针真实存在 | `git ls-remote origin` | `probe/anolis-roundtrip = b5c6f1b` 在 |
| 硬编码密钥模式扫描 | `grep -rnE 'ghp_\|glpat-\|sk-\|xai-\|AKIA' af/ 计划/ 状态/ 设计/` | 无真实令牌命中（botocore 示例数据里的 AKIAIOSFODNN7EXAMPLE 是 AWS 文档示例串，非真实凭据） |
| anolis 横向直通 | `ssh anolis 'ls ~/.ssh/'` | 只有 gitea 一把钥匙 + known_hosts（127.0.0.1 + 一个 hashed 主机）——**未发现到 gov/hk 的 root 私钥**，auto-approve 实例不能直接 ssh 进信任层 |

## 4. 未跑 / 无法核实（不填充猜测）

1. **CI 六门中的 clippy / cue / scrub / cosign 四门本轮未执行**（只跑了 fmt + test 两门，外加不在六门内的 deny 三类）。交叉审 MiniMax 核实了六门结构齐全（run.sh:75,80,85,90,122,147）。
2. **"已推远端"（F1 的备份声称）无法核实**：`/home/anuser` 对 pan-ding Permission denied，anolis `sudo -n` 需密码；未登录 gov 核查 gitea 服务端配置（TLS/暴露面/令牌清单）——只读范围内未做。
3. **17:20 grok 额度到点后的实际行为未观察**（审查在 16:10 结束，届时额度未到期）。
4. **是否已发生实际凭据外泄无证据**：logs 扫描未发现 token 模式，但 TUI 转录含 agent 读过的任意文件内容，历史日志（9 份、3.1M）只做了模式扫描，未逐字审计。
5. **/tmp 3.1G 占用方未查**（超出只读审查范围）。

## 5. 给主代理的处置优先级建议（红队视角，非执行）

1. （今天 17:20 前）grok 额度到期前确认 9 实例的落盘状态与共享 target 锁；过期后先 `ps` 清查再动 target。
2. （可逆性优先）F1：补登 30 号文件删除记录；找到"已推远端"的实际位置并做一次完整性核验；若核验不了，按数据丢失预案处理。
3. F2：私钥立即按"已泄漏"对待——不用于任何真实签名；history 清理（filter-repo + 镜像 purge + 全员 re-clone）排期；3191 个 target 文件从索引移除（`git rm -r --cached` 后提交，.gitignore 已覆盖新增）。
4. F4：给 11 个 crate 补 `license`（workspace 继承）；对 RUSTSEC-2026-0173 做显式裁决（接受并 ignore + 记录理由，或换 biscuit 链）；把 `cargo deny --offline check …` + db 预置写进 ci/run.sh 当第七门。
5. F3：`.git-token` / SSH 私钥 / auth.json 与实例身份隔离（专用低权用户或 per-instance 短时令牌）；`--always-approve` 至少加命令黑名单（凭据路径、网络外传）。
6. F5：1.99.1 pin 保留可以，但 CI step0 的 `rustup component add` 加 `--offline` 优先/失败即明确报错；考虑钉一个 soak 过的版本而非 4 天前的。
7. F6：gitea 镜像改为每次 main 合流即推（或 gov→pandev-comp 每日快照），并评估 gitea/bao 不同宿主拆分。

---

*报告完。所有命令均在 2026-10-19 15:40–16:10 于 pandev-comp（100.64.0.8）执行；anolis/hk 只读 SSH 命令均带 BatchMode。未执行任何 git commit / push，未修改 af 仓库任何文件（唯一写入为本报告）。*
