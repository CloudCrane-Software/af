# af-engine 版本切换

引擎进程无状态：一行请求、一行响应、然后退出，内存里不留下一张订单，也不留下「这个 run 用过哪一版」。`engine_version` 在编译期钉进那一份二进制，运行时的 `CARGO_PKG_VERSION` 改不了响应。

所以版本切换发生在编排选择哪一份二进制。通道上同时放两份，用指针决定新 run 进哪一份。

## EngineChannel

```text
EngineChannel {
  stable: 路径    // 当前放量的二进制。槽位保持有值。
  canary: 路径?   // 灰度二进制。空表示当前没有灰度。
}
```

两份二进制同时留在磁盘上。`stable` 与 `canary` 是指针（符号链接），本体放在 `bins/` 里，指针指过去。灰度期间不覆盖 `stable` 正在被旧 run 使用的那个文件。

`ENGINE_CHANNEL_DIR` 布局：

```text
$ENGINE_CHANNEL_DIR/
  stable                      # 符号链接，指向 bins/ 里的稳定本体
  canary                      # 符号链接。文件在 = 灰度开启；文件不在 = 没有 canary
  bins/<engine_version>-<id>  # 二进制本体。<id> 区分同版本的两次构建
  pins/<run_id>               # 一行文本：该 run 锁定的二进制绝对路径
```

`ENGINE_BIN` 是刚编出来、还没挂上指针的那份文件。`ENGINE_BUILD` 是 `bins/` 下的目录名，取响应里的 `engine_version` 再加构建标识。

## 选择规则

1. 新规则先发 canary。
   新的判定行为，或调用方提高了 `engine_version_min`，先编出新二进制，写入 `bins/`，再把 `canary` 指到它。`stable` 保持不动。
   只有在第一次引擎调用之前被编排标成 canary 的新 run，才读 `canary` 指针。其余新 run 读 `stable`。
   标了 canary 但指针当时不存在：拒绝这次调用，不把该 run 悄悄改接到 `stable`。新规则要的是新二进制；旧二进制会在版本信封上拒绝低于下限的请求，这里连调用都不发出去。

2. 旧 run 锁旧版。
   引擎无状态，进程里锁不住 run。锁的是编排约定：该 run 全程用同版本。
   run 第一次调用引擎时，编排用 `readlink -f` 把当时选中的指针解析成绝对路径，写入 `pins/<run_id>`。此后这个 run 的每一次调用都执行 pin 里的那条路径，不再读 `canary` 或 `stable` 指针。
   指针后来指向别的文件，或被摘掉，已经写下的 pin 保持原样。

3. 回滚就是摘除 canary 指针。
   删除 `$ENGINE_CHANNEL_DIR/canary` 这个符号链接。不删除 `bins/` 里的本体，也不改任何 `pins/<run_id>`。
   摘除之后，还没有 pin 的新 run 不再进入灰度，回到 `stable`。编排同时停止把新 run 标成 canary。
   已经把旧 canary 路径写进 pin 的 run 继续用那份本体，直到 run 结束。没有任何 pin 再引用某份本体时，才删除 `bins/` 里的对应文件。

## 和版本信封的关系

线协议见 `README.md`。请求可以带 `engine_version_min`。这份二进制的 `engine_version` 低于它时，进程拒绝判定，message 提示升级，响应里仍然带上自己的 `engine_version`。看不懂的下限也拒绝，不当成没有下限。

因此新规则的请求即使打到旧二进制上，旧二进制也不会用旧规则把订单判过去。编排在写 pin 之前核对响应里的 `engine_version`。与这次选择的预期不一致时，不把该路径写入 pin。

## 操作步骤

下面这一组命令是发布 canary 与回滚的全部文件操作。干跑入口是 `scripts/engine-channel-dry-run.sh`：它只对每条命令做 `command -v`，不编译、不复制、不摘指针。

```sh
cargo build -p af-engine --bin af-engine
mkdir -p -- "$ENGINE_CHANNEL_DIR/bins" "$ENGINE_CHANNEL_DIR/pins"
cp -f -- "$ENGINE_BIN" "$ENGINE_CHANNEL_DIR/bins/$ENGINE_BUILD"
ln -sfn -- "$ENGINE_CHANNEL_DIR/bins/$ENGINE_BUILD" "$ENGINE_CHANNEL_DIR/canary"
readlink -f -- "$ENGINE_CHANNEL_DIR/canary"
test -x "$ENGINE_CHANNEL_DIR/canary"
rm -f -- "$ENGINE_CHANNEL_DIR/canary"
test ! -e "$ENGINE_CHANNEL_DIR/canary"
```

各步的含义：

1. `cargo build` 编出待灰度的二进制。产物先作为 `ENGINE_BIN`，不写入 `stable`。
2. `mkdir` 准备 `bins/` 与 `pins/`。
3. `cp` 把新二进制放进 `bins/<engine_version>-<id>`。这是本体。
4. `ln -sfn` 把 `canary` 指针指到本体。从此，被标成 canary 且尚未写 pin 的新 run 会选中它。`stable` 不变。
5. `readlink -f` 得到绝对路径。编排把这一行写入 `pins/<run_id>`。该 run 之后只执行这一条路径。
6. `test -x` 确认当前指针指向一个可执行文件。
7. `rm -f` 摘除 canary 指针。这就是回滚。
8. `test ! -e` 确认指针已经不在。`bins/` 里的本体还在，已有 pin 继续可用。

安装 `stable` 用同一次 `cp`，再把链接目标写成 `$ENGINE_CHANNEL_DIR/stable`。替换 `stable` 指针之前，先确认没有 pin 还指向即将被换掉的那份本体；仍有引用时保留旧文件。灰度步骤本身不改 `stable`。
