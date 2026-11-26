# VIctory-macOS 在 M5 Max 上的第一幕对照

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

使用 fork 的原始代码构建同一份 release 程序，对比上游的默认行为和写入跟踪设置。每次启动使用单独的 Wine 配置、存档副本、着色器缓存副本、游戏工作目录和截图目录。只读游戏资产来自本机已有副本。主工程的材质启动器包含额外的渲染改动，本实验直接运行上游的 Series S 路径。

## 本机结果

两组都进入了上游按绘制数判定的世界阶段。取进入后的第 2 张到第 8 张截图，共六个连续交付间隔。

| 同一版本 c78cfdc | 默认 | 启用写入跟踪 |
|---|---:|---:|
| 完整交付间隔均值 | 7,716 ms | 2,294 ms |
| P95（最近秩，本窗口等于最大值） | 9,533 ms | 4,332 ms |
| 每帧拷贝量，日志采样中位 | 71,762 MB | 559 MB |
| 每帧拷贝耗时，日志采样中位 | 7,010 ms | 407 ms |
| 实际绘制数，日志采样中位 | 1,645 | 2,360 |
| 拷贝及绘制的日志样本数 | 6 | 2 |

第 8 张截图在两次运行中像素均为全黑。两组此前均出现“VICE CITY, TWO YEARS AGO”字卡。默认组继续交付至第 12 张；跟踪组在第 8 张后另观察了 69 秒，至 185 秒的上限没有再收到一张。此次测试中跟踪版的复制量、复制用时和交付间隔均下降，但画面和连续运行未达第一幕游玩要求。固定对照的帧耗时、带时间戳的性能样本及 PNG 像素散列见 [mac-pair.json](<../../../evidence/forks/mac/mac-pair.json>)。

## 复现

```sh
python3 build.py
python3 run.py default
python3 run.py tracking
python3 summarize.py
```

运行顺序为默认设置、启用跟踪。两次都使用 GPTK 4、同一组定时按键和最多 185 秒的观测窗口。上游以单帧至少 500 次绘制标记进入世界。首次标记可能仍在显示第一幕字幕。程序到世界之后保存连续截图；对相邻截图记录的完成时间作差，得到包含抓图开销的交付间隔。性能日志另外给出拷贝耗时、拷贝量、实际绘制数和丢弃数。由于两次执行的输入时机、画面和绘制量会浮动，这些数字不能直接归因于单项开关。我们也记录加载阶段相同帧号区间的总用时。

每次执行会在此目录创建独立子目录。配置与二进制散列在 run.json，原始日志在 game.log，截图在 frames。results.json 汇总各次测试，保留每个样本的时间戳和帧号。截图 SHA-256 对应解压后的原始 RGB 像素。

其他已构建分支可通过 `--binary`、`--source-repo`、`--exe-label` 和 `--set VICTORY_KEY=VALUE` 接入相同的运行流程。`--fxc-dll` 为本次副本提供原生 d3dcompiler_47.dll。`--mullion-runtime` 支持已单独安装 Mullion 的 CrossOver 目录，并为每次启动分配独立的着色器缓存目录。
