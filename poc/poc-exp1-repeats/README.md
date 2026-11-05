# poc-exp1-repeats —— EXP-1 repeat 轮（A0/A1/A2 第二跑 72 单）补齐件

> 债号：P3-B1 残项（EXP-1 repeats 72 数据补齐）
> 分支：`feature/exp1-repeats-backfill`
> 口径正本：**`口径-EXP1-repeats-v1.md`**（先读它；本 README 只是索引）
> 预注册正本（只读、零改动）：`计划/研究组/B线核心批-20261029/experiments-v1.json` EXP-1 段

## 目录

| 文件 | 作用 |
|---|---|
| `口径-EXP1-repeats-v1.md` | **执行口径 + 分析口径 + 成本口径 + 偏差登记**（本件正本） |
| `exp1-repeats-submit.sh` | 72 单提交脚本（7 项预检 + 并发 4 + 失败留痕 + 末尾汇总） |
| `exp1-repeats-mkspec.py` | af-order spec 生成（repeat 轮 note 标注 round=repeat attempt=2） |
| `exp1-repeats-aggregate.py` | 逐单提取 + 真实性锚 + 数据门 + repeat 轮内统计 + flakiness + 探索性 pooled + 成本；含**主轮口径对拍**模式（`--selfcheck-only --regress-main`） |
| `exp1-repeats-armed-run.sh` | **自动起批段**（anolis 常驻）：等配额窗 → 起批 → 早停闸（连续 429 杀批重试）→ 落 DONE/GAP 标记 |
| `exp1-repeats-land.sh` | **自动落数段**（本机常驻）：等 DONE 标记 → 回传镜像 → 对拍门 → 聚合 → 落 data/ → commit+push |
| `pkg-order-r2.txt` | 72 包清单（= 主轮 `pkg-order.txt` sha256 `41e8e265…c28d7` 按臂过滤，分配表顺序） |
| `evidence/attempt1-quota429/` | **试跑阻断证据**（16 份失败 `.result` + 提交日志 + 单单探针账本/日志） |
| `data/` | 补跑落盘后的读数（per-run 明细 jsonl + 汇总 json + 落数日志）——**补跑成功后由落数段自动写入** |

## 现状（一句话）

harness 全在位且亲验通过；**唯一硬闸是 glm-cp 通道配额窗**——2026-11-06 00:52–01:01 实测
provider 错误码 1310「已达到每周/每月使用上限」，限额 **2026-11-06 14:16:36** 重置；72 单批
自身墙钟 ≈2.2 h（并发 4）⇒ 数据落盘必然晚于短会话，故配了自动起批 + 自动落数两段常驻脚本。
详见口径件 §七（偏差登记）与 §八（自动化排期）。

## 复现（原件可复跑）

```bash
# 提交（anolis，pan-ding 身份）
cd /data/bench/exp1 && bash exp1-repeats-submit.sh

# 口径对拍（先决门，不 PASS 不许落数）
python3 -I af/poc/poc-exp1-repeats/exp1-repeats-aggregate.py --selfcheck-only \
  --regress-main 计划/研究组/B线核心批-20261029/exp1-aggregate-summary.json \
  --main-mirror ~/exp1-raw/mirror/orders

# 聚合出读数
python3 -I af/poc/poc-exp1-repeats/exp1-repeats-aggregate.py \
  --mirror ~/exp1-raw-r2/mirror/orders --main-mirror ~/exp1-raw/mirror/orders \
  --exp1-dir 计划/研究组/B线核心批-20261029 --out-dir af/poc/poc-exp1-repeats/data
```

## 纪律

- 预注册件（`experiments-v1.json` / `exp1-allocation.json` / `exp1-materials.json` /
  `exp1-lock.json`）与主轮正本（`exp1-aggregate-summary.json` / `exp1-结果判读-v1.md`）**零改动**；
- 判定程序唯一实现 `tools/b-verdict-unique.py` 不复制、不改写；本件**不签发 EXP-1 主判定**；
- af 仓 Rust/DSL 代码零改动（预注册 `execution.zero_code_change=true`）；
- 密钥零明文（提交脚本只报 key 长度：`KEY_OK len=49`）。
