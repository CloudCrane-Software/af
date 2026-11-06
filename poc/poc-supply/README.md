# PoC-G 供给画像（配方层，无内核新增）

## 组件

- `af/data/recipes/user-01/machines.v1.yaml`：四节点静态注册表。只含行政状态、角色、信任级、硬件基线、磁盘阈值和供给通道声明；不含 `ssh_ok`、实时水位或健康值。
- `validate_machines.py`：PyYAML SafeLoader 的受限子集（禁锚点/别名/显式 tag/重复键），并执行 `deny_unknown_fields`、必填、bytes 整数、禁 latest/秘密/绝对路径/内联脚本及 PoC-G 语义校验。
- `collect_resource_observation.py`：本机直读与远端 SSH BatchMode 只读采集，原子写时间戳 JSON，并原子更新相对符号链接 `latest.json`。
- `disk_post_used_preflight.py`：按一个 `节点/逻辑挂载` 计算 `post_free=avail-Σ未过期 active lease 预留-request`，同时检查最小投后余量与最大投后使用百分比。退出码：0=投放，2=拒单，1=输入/读取错误（fail closed）。
- `lease_store.py`：SQLite lease 元组 `(resource, work_interval, profile, ttl_ms)`，实现 `acquire`、`return`、`reap`，另带 holder/TTL 守卫的 `heartbeat` 与 `show`。

运行态固定落在仓库 `af-state/resources/`，由根 `.gitignore` 排除；不写入静态配方。

## 逻辑挂载与路径纪律

配方禁止绝对路径，所以 `mount` 是逻辑键，不是路径：采集器内部固定映射 `root→/`、`data→/data`、`tmp→/tmp`。`write_roots` 是相对各挂载根的运营声明。预检和 lease 使用 `anolis/root`、`anolis/data` 等不同资源键，不能把 data 盘余量算到根盘。

## 执行

以下命令均从本工作树执行，无需 sudo：

```bash
ROOT=/home/pan-ding/workspace/af-wt-pocg
REGISTRY="$ROOT/af/data/recipes/user-01/machines.v1.yaml"
DB="$ROOT/af-state/resources/leases.db"

python3 -I "$ROOT/poc/poc-supply/validate_machines.py" "$REGISTRY"
python3 -I "$ROOT/poc/poc-supply/collect_resource_observation.py" --registry "$REGISTRY" --node-id pandev
python3 -I "$ROOT/poc/poc-supply/collect_resource_observation.py" --registry "$REGISTRY" --node-id anolis

python3 -I "$ROOT/poc/poc-supply/lease_store.py" --db "$DB" acquire \
  --lease-id example-anolis --resource anolis/root --interval worktree:example \
  --profile '{"schema_version":"af.resources/v1","recipe_namespace":"user-01","node_id":"anolis"}' \
  --holder pocg --reserved-bytes 1073741824 --ttl-ms 600000
python3 -I "$ROOT/poc/poc-supply/disk_post_used_preflight.py" \
  --registry "$REGISTRY" --observation "$ROOT/af-state/resources/observations/anolis/latest.json" \
  --leases-db "$DB" --node-id anolis --mount root --request-bytes 1073741824
python3 -I "$ROOT/poc/poc-supply/lease_store.py" --db "$DB" return \
  --lease-id example-anolis --holder pocg
python3 -I "$ROOT/poc/poc-supply/lease_store.py" --db "$DB" reap
```

测试：

```bash
python3 -I "$ROOT/poc/poc-supply/tests/test_registry_validation.py"
python3 -I "$ROOT/poc/poc-supply/tests/test_disk_preflight.py"
python3 -I "$ROOT/poc/poc-supply/tests/test_lease_store.py"
```

## 静态基线来源与边界

- pandev/anolis CPU 与 MemTotal 在本次施工由 `nproc`、`/proc/meminfo` 实测；观测文件保存当前动态读数。
- gov/mingda 静态硬件沿用 `状态/机器清单.md` 的 8 vCPU/15.6G、4 vCPU/7.4G 基线，按 GiB 转为最近整数 byte；本次未 SSH 采集这两台，未声称有新鲜健康读数。
- 本机 ollama 通道固定为 `http://127.0.0.1:11434`；anolis 不登记该供给。B 类沙箱和 step 均禁用且重启须运营者裁决；kimi 为 `restricted`。
- 所有首版并发上限计量均为 `Stock`，不使用已知嵌套扣减失效的 `Concurrency`。`allocatable=null` 表示尚未由动态观测与 lease 推导，不是零供给。
- 采集器是一次一采的 PoC 工具，本次未安装 systemd/cron 守护。
