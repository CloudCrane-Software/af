# poc-r5-crosstree — strategies_check R5 跨树集合比较腿（RT-E82 厂内重建批）

批次：施工·R5跨树集合门（2026-11-05 夜，anolis）
分支：`feature/r5-crosstree-set-gate`（自 `origin/product/main` 2de4453 起）
工作树：`/data/bench/wt-r5-crosstree`（anolis）
规格来源：rt-e82 submission summary（归档件可读、代码勿抄；本批实现全自写）

## 1. 改什么

`ci/strategies_check.py` 新增 R5 跨树集合比较腿（既有 R0-R4 判定一字未动，
diff 对原件的删除行仅 4 行输出串：模块 docstring 退出码行、红计数行、两条绿行
——均为把 R5 纳入陈述，无任何断言弱化）：

- `_worktree_digests()`：工作树 `strategies/` 全部文件 → `{仓内相对路径: sha256}`
  （集合口径同 R0 盘面集合：含 index.yaml 与非条目辅助文件；排序遍历保复现）。
- `_git_head_digests()`：`git ls-tree -r -z --name-only HEAD -- strategies` 取
  路径集，逐文件 `git show HEAD:<rel>` 取内容后算 sha256（与工作树侧同一种摘要）。
- `_manifest_digests()`：非 git 切片回退 `MANIFEST.json` 的 `files[]`
  （候选：`AF_CROSSTREE_MANIFEST` 环境变量 → 仓根 → 父目录）。
- `_crosstree_baseline()`：git 提交树优先，manifest 回退，两者皆无 = 无法判定。
- `check_crosstree()`：四类差异逐条指名、非空即硬红：
  `only-in-worktree`（新增/未入库）/ `only-in-head`（缺失/旧树）/
  `content-diff`（同名内容改写，输出两侧 sha256 前 12 位）/
  `改名漂移`（only 两侧同摘要配对点名）；无差异才 INFO 一致读数；
  无基线 fail-closed 红（不静默放行）。
- 挂点：`main()` 内 `check_durability()`（R0）之后调用 `check_crosstree()`；
  不新增 `ci/run.sh` step——长在既有 step14 段尾 14d 的 strategies_check.py 内
  （run.sh:596 `python3 -B ci/strategies_check.py` 只取退出码，输出串改动无机读依赖）。

动因（规格复述）：R0 的 `_git_tracked`（git ls-files）只判「被测对象在跟踪面」，
`_head_files`（git ls-tree HEAD）仅用于 INFO 提醒——「跟踪面有」≠「与提交树
一致」：已 add 未 commit 的改写、工作树删件、未入库新增件都能让门对着与 HEAD
不同的集合照绿。R5 把 HEAD 提交树集合升格为判定基线。

## 2. 怎么复现

```sh
ssh anolis 'cd ~/af-dev/af && git fetch origin && \
  git worktree add /data/bench/wt-r5-crosstree -b feature/r5-crosstree-set-gate origin/product/main'
# 本目录 fixture_r5_crosstree.py 即夹具；日志在 logs/
ssh anolis 'export PATH=$HOME/.cargo/bin:$PATH && cd /data/bench/wt-r5-crosstree && \
  CARGO_TARGET_DIR=/data/bench/targets/r5-crosstree \
  python3 -B poc/poc-r5-crosstree/fixture_r5_crosstree.py'
```

夹具每个用例 `finally` 强制还原 strategies/ 到 HEAD 态，开跑前与 G8 收尾均校验
`git status --porcelain -- strategies` 为空。

## 3. 夹具矩阵与实测（logs/ 内留全量输出）

| 用例 | 构造 | 预期 | 实测 |
|---|---|---|---|
| G0 基线一致态 | 全量门（含 R4 cargo） | exit 0 + R5 一致 INFO | 见 logs/G0-baseline-green.log |
| V1 worktree-only | 新增 strategies/zz-r5-fixture-stray.txt 并 git add | exit 1，指名 only-in-worktree | logs/V1-worktree-only.log |
| V2 head-only | git rm strategies/README.md（非条目辅助文件，单腿隔离） | exit 1，指名 only-in-head，且仅 1 条 FAIL（不误伤 R1-R3） | logs/V2-head-only.log |
| V3 content-diff | 追加一行注释到 strategies/README.md | exit 1，指名 content-diff，单腿红 | logs/V3-content-diff.log |
| V4 改名漂移 | git mv README.md → README-r5-moved.md | exit 1，指名两侧 only + 漂移配对 | logs/V4-rename-drift.log |
| V5a 条目级新增 | 新增合法条目 st-b-C999.yaml（暂存） | exit 1，指名 only-in-worktree（伴随 R2 未登记红属预期；R3 只对 index 登记面机算，不随磁盘新增漂移） | logs/V5a-entry-new.log |
| V5b 条目级缺失 | git rm st-C005.yaml（index 已登记） | exit 1，指名 only-in-head（伴随 R2 缺席红属预期） | logs/V5b-entry-rm.log |
| V6 非 git 切片 | 拷贝树去 .git、无 MANIFEST | exit 1，fail-closed 指名，诊断含双源不可用 | logs/V6-nogit-failclosed.log |
| V7 非 git 切片 | 同拷贝树 + 仓根 MANIFEST.json（HEAD 65 件 sha256） | exit 0，R5 一致且基线来源=MANIFEST | logs/V7-manifest-fallback.log |
| V7b 环境变量 | MANIFEST 经 AF_CROSSTREE_MANIFEST 定位 | 同上 | logs/V7b-manifest-env.log |
| G8 还原复绿 | 全量门两次 | exit 0；两次输出逐字节一致 | logs/G8-restored-green.log、G8-twice-run-a/b.log |

V2/V3 选材说明：`strategies/README.md` 在 HEAD 内、非 st-*.yaml 条目、不被
index.yaml 登记，R1（glob st-*.yaml）/R2/R3（只对登记条目）均不触碰，故可单腿
隔离取证；V5a/V5b 用条目 YAML 演示条目级漂移同样被点名（此形态 index 登记面同步
失效，R2/R3 红属预期，夹具不断言其缺席）。

## 4. 门面读数（本批实跑）

- 全量门（anolis 工作树，冷建 target）：
  `CARGO_TARGET_DIR=/data/bench/targets/r5-crosstree python3 -B ci/strategies_check.py`
  → exit 0，末行 `== strategies_check: 全绿——磁盘条目 61（compilable=44），index 计数对账一致，R5 跨树集合一致，编译断言经 af-dsl 装载测试`。
- af-dsl 快面：`cargo test -j4 -p af-dsl --lib`（env -u 代理变量 + LC_CTYPE=C.UTF-8 前缀）
  → 见 logs/cargo-af-dsl-lib.log。

## 5. residue（全量门待跑声明）

本批只跑了 strategies_check 单门（step14 段尾 14d 的脚本本体）与 af-dsl --lib
快面；**未跑** ci/run.sh 全量门（step1-14 全链）/ all-gates 全量面——R5 随
step14d 挂接后的全链联动读数（含 xcheck 门数对账、numbers.json 三键不受影响
的验证）留待全量门批次执行。另：本批未合线，仅 push 分支。
