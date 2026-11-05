# poc-a3-rte138-keypromo-v2 — A3 键推广归因与断言勘正

日期：2026-11-05　分支：`feature/a3-rte138-keypromo-v2`　基线：`origin/product/main` = `59403d1`

前批 `feature/a3-rte138-keypromo`（`08b29ee`）独立终审裁不可收卷。本包是三项必修的自足留证。CH 链路 / 投影器 / `otlp-assert.py` **零改**。

## 必修 1 · 归因勘正（+29 不是 Rust 键推广）

### 链路（亲读，未改）

| 面 | 谁写 | 消费者 |
|---|---|---|
| 本地状态面 JSON `af.otel.production.v1` | Rust `span_produce::SpanRow`（本批补 `work_order_id`） | **无消费者入 CH** |
| CH `otel.otel_traces` 订单域行 | `tools/otel/af-otel-projector.py` `:408` / `:437` 写 `af.work_order_id=stable_id`（首行 `:90`）；经 `emit-semantic-otlp.py` → collector | P2 判据输入面 |

A3 未动投影器。Rust 侧 `order_domain` 键进入 CH 的效果**当前不可直接观测，依赖投影器侧**。

### +29 三单逐单核平（2026-11-05 14:00 窗）

CH 真查 `raw/ch-plus29-reconcile.json`（工厂 `ch_client`，窗 `14:00–15:00`，`af.order_id != ''`）：

| 单 | n | service.name | 与前窗关系 | 归因 |
|---|---|---|---|---|
| `F1-swev-django-11099-glm-cp` | 20（投影器 19 + harvest 1） | `af-otel-projector` / `sandbox-harvester` | 07:44 已有 20 行，span_id 重叠 **19** | **重放重复行**（RT-E138 接通 CH 时已入过） |
| `F5c-report-featureflag-2610-31351-glm-cp` | 7 | `af-otel-projector` | 14:00 当日首次入窗；15:00 同 7 个 span_id 再入 14 行 | 投影器重放；15 时再次重复 |
| `wo-span-3788989` | 2（L1 + harvest） | 同上 | 14:30 A3 测试 stub 经 `emit-semantic-otlp.py` | 仍走投影器，**不是** Rust `SpanRow.work_order_id` |

合计 **20+7+2 = 29**。`other` service.name = 0。Rust 生产者 0 行入 CH。

前批把 14:00 窗 +29 写成「Rust 键推广致 CH 键行上升」——**不实**。CH 行来源=投影器；+29=三单经发射脚本重放/重复入库。

## 必修 2 · 撤回「同一真源同一值」（选项 b）

终审证伪：Rust 取 `stable_id` 集合最小值，投影器取首行，漂移/首行缺键时值不等。

本批选 **(b)**：不改两侧算法、不改 CH 链路。ADR-003 与 `span_produce.rs` 注释改写为「两侧独立取键，容许漂移，判据面以投影器侧为准」。

漂移探针（WARN，不改 P2 红绿）：

- Rust：`probe_key_drift` / `count_key_drift_warns`；不等则 L1 `attrs.ledger_key_drift=true`
- 夹具：`key_drift_probe_warns_when_first_row_differs_from_set_min`（首行 `sid-zzz` / 其余 `sid-aaa` → WARN=1，本面写 `sid-aaa`）
- Python 镜像：`raw/key-drift-probe.py`（工厂 `otlp-assert.py` 零改 = 判据面零弱化）

## 必修 3 · 缺 stable_id：注释改写 + fail-closed

修前（`origin/product/main` `af-trace-tree-verify --publish`，夹具 `raw/fixture/missing-sid-ledger.jsonl`）：

| 项 | 读数 | 文件 |
|---|---|---|
| tracetree `stable_id` | 空 | `raw/before/missing-sid-tracetree.txt` |
| findings 含「缺 stable_id」 | **0**（6 条全是 MES/records） | 同上 |
| publish | **rc=0，1 行** `order_id` 有、`stable_id=""`，无 `work_order_id` | `raw/before/missing-sid-state.json` |

前批注释「tracetree 核对器另报 finding」为假；该路径产出半键行，与「每行两键齐全」矛盾。

修后（本分支 `project_order` 在 `report.stable_id` 为空时不产出行）：

| 项 | 读数 | 文件 |
|---|---|---|
| tracetree findings 仍不含缺 stable_id | 0（核对器未改） | `raw/after/missing-sid-tracetree.txt` |
| publish | **rc=1，零行，状态面不落盘** | `raw/after/missing-sid-publish.txt` |
| 测试 | `missing_stable_id_emits_zero_rows_fail_closed` | — |

## 复放

```bash
# 缺 stable_id 夹具（修后应 publish rc=1）
cargo build -p af-execline --bin af-trace-tree-verify
./target/debug/af-trace-tree-verify --publish /tmp/state.json \
  <(echo; true)  # 或复制 raw/fixture 为订单目录后指向它

# 漂移探针
python3 -I poc/poc-a3-rte138-keypromo-v2/raw/key-drift-probe.py <order_dir>
```

## 边界

- 工厂投影器 / 断言器 / CH schema 零改。
- `emit-semantic-otlp.py` 零改（CH 发射仍走投影器）。
- 漂移时本面按 Rust 集合最小写 `work_order_id`；CH 仍按投影器首行。两面可以长期不等。
- 缺 `stable_id` fail-closed 只约束**本状态面**；投影器仍可能对首行空键落 CH 半键行（P2 管 CH）。
