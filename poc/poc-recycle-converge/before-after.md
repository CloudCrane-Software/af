# 改前/改后行为对照（poc 证据）

- 改前脚本取回：`git show 66be37c:tools/worktree-recycle.py`（基线=origin/product/main）
- 改后脚本：本批 tools/worktree-recycle.py
- 驱动器：behavior-diff.py（同夹具同命令，仅换脚本）；git 2.47.3 / python3 -I

## 改前（66be37c 原样）（script=/tmp/recycle-poc/orig-worktree-recycle.py）
### E1 脏树（真合入 merged + f.txt 未提交改动）
```
CLEAN feat: merged 且已真合入 → 清（已移除 worktree /tmp/recycle-poc-1n6_tvjh/e1/.af-worktrees/feat；已删分支 feat）
```
  - 树目录: 已被删
  - 未提交改动: （目录已不存在）
  - 分支 feat: 已被删
  - feat 提交对象 cat-file -t: commit
### E2 未合入分支（aborted 已过 48h 留证期）
```
CLEAN feat: aborted 留证 49.0h ≥ 48.0h → 清（已移除 worktree /tmp/recycle-poc-1n6_tvjh/e2/.af-worktrees/feat；已删分支 feat）
```
  - 树目录: 已被删
  - 未提交改动: （目录已不存在）
  - 分支 feat: 已被删
  - feat 提交对象 cat-file -t: commit
### E3 merged 干净树（默认，无任何开关）
```
CLEAN feat: merged 且已真合入 → 清（已移除 worktree /tmp/recycle-poc-1n6_tvjh/e3/.af-worktrees/feat；已删分支 feat）
```
  - 树目录: 已被删
  - 未提交改动: （目录已不存在）
  - 分支 feat: 已被删
  - feat 提交对象 cat-file -t: commit
### E4 误登记路径（--path 指向普通目录 innocent-dir/，内有无关文件）
```
CLEAN stray: aborted 留证 49.0h ≥ 48.0h → 清（worktree remove rc=128，已兜底删目录 /tmp/recycle-poc-1n6_tvjh/e4/innocent-dir）
```
  - innocent-dir: 已被删
  - 无关文件.txt: 已被删

## 改后（feature/worktree-recycle-converge）（script=tools/worktree-recycle.py）
### E1 脏树（真合入 merged + f.txt 未提交改动）
```
KEEP feat: 目录移除失败 → 留检保现场（worktree remove 失败（fatal: '/tmp/recycle-poc-p3glep9o/e1/.af-worktrees/feat' contains modified or untracked files, use --force to delete it）→ 留检保现场，未删目录 /tmp/recycle-poc-p3glep9o/e1/.af-worktrees/feat；脏面摘要：M f.txt）
```
  - 树目录: 仍在
  - 未提交改动: 保留，内容='base\n未提交的活\n'
  - 分支 feat: 仍在
  - feat 提交对象 cat-file -t: commit
### E2 未合入分支（aborted 已过 48h 留证期）
```
CLEAN feat: aborted 留证 49.0h ≥ 48.0h → 清（已移除 worktree /tmp/recycle-poc-p3glep9o/e2/.af-worktrees/feat；保留分支 feat（默认保分支保提交））
```
  - 树目录: 已被删
  - 未提交改动: （目录已不存在）
  - 分支 feat: 仍在
  - feat 提交对象 cat-file -t: commit
### E3 merged 干净树（默认，无任何开关）
```
CLEAN feat: merged 且已真合入 → 清（已移除 worktree /tmp/recycle-poc-p3glep9o/e3/.af-worktrees/feat；保留分支 feat（默认保分支保提交））
```
  - 树目录: 已被删
  - 未提交改动: （目录已不存在）
  - 分支 feat: 仍在
  - feat 提交对象 cat-file -t: commit
### E4 误登记路径（--path 指向普通目录 innocent-dir/，内有无关文件）
```
KEEP stray: aborted 目录移除失败 → 留检保现场（worktree remove 失败（fatal: '/tmp/recycle-poc-p3glep9o/e4/innocent-dir' is not a working tree）→ 留检保现场，未删目录 /tmp/recycle-poc-p3glep9o/e4/innocent-dir；脏面摘要：?? .af-worktree-registry.json；?? innocent-dir/）
```
  - innocent-dir: 仍在
  - 无关文件.txt: 仍在
