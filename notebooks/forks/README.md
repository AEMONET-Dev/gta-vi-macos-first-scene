# 第一幕开源渲染实验

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

在 Apple M5 Max 上分别测了游戏整帧、GPU 调度、单道计算和单个资源的搬运。30 FPS 的预算为每个完整画面 33.3 ms。所有代码来自 [6 个已核验的 GitHub fork](<../../evidence/forks/forks.json>)，本地 checkout 在 repos。创建 fork 时只复制了上游公开内容，实验的游戏输入、读回结果和日志存于本机。

## 已 fork 的源码

| GitHub 副本 | 上游作者或项目 | 本地版本 |
|---|---|---|
| [VIctory-macOS](https://github.com/AEMONET-Dev/VIctory-macOS) | [pragyaangaur](https://github.com/pragyaangaur/VIctory-macOS) | `c78cfdc46` |
| [victory-perf](https://github.com/AEMONET-Dev/victory-perf) | [vitutu1](https://github.com/vitutu1/victory-perf) | `97c3aa242` |
| [mullion](https://github.com/AEMONET-Dev/mullion) | [dttdrv](https://github.com/dttdrv/mullion) | `11e77a859` |
| [dxil-spirv](https://github.com/AEMONET-Dev/dxil-spirv) | [HansKristian-Work](https://github.com/HansKristian-Work/dxil-spirv) | `694cc6743` |
| [SPIRV-Cross](https://github.com/AEMONET-Dev/SPIRV-Cross) | [KhronosGroup](https://github.com/KhronosGroup/SPIRV-Cross) | `57e4efcc8` |
| [xenia-edge](https://github.com/AEMONET-Dev/xenia-edge) | [has207](https://github.com/has207/xenia-edge) | `0b12401d8` |

## 同源 Mac 版本

同一份 VIctory-macOS 可执行文件、Series S 配置、同一组定时输入，每轮从各自的缓存和存档副本启动。表中两次各取世界场景计数开始后 +2 到 +8 的六个相邻截图间隔。截图完成时间包含截帧成本。两轮每帧执行的绘制量不同。

| 实际游戏测量 | 默认 | 启用上游写入跟踪 |
|---|---:|---:|
| 完整画面间隔平均 | 7,716 ms | 2,294 ms |
| 六个间隔中最慢 | 9,533 ms | 4,332 ms |
| 拷贝量中位数 | 71.76 GB/帧 | 0.56 GB/帧 |
| 拷贝耗时中位数 | 7,010 ms/帧 | 407 ms/帧 |
| 窗口内拷贝计数器样本 | 6 | 2 |

均出现了第一幕字幕，但 +8 帧两次都全黑。跟踪版在后续 69 秒未交付下一张截图。复用写入跟踪具有减少搬运的价值；验收时还要看到能持续更新的场景。[两次完整对照](<../../evidence/forks/mac/mac-pair.json>) · [配置、帧号与像素校验](<mac/README.md>)

## 指令调度和着色器

- **victory-perf** 的两个开关将不必要的全 GPU 等待限制在参与本次命令的缓冲。使用第一幕五条真实输入和计数着色器，16,000 次 GPU 调度的计数完全正确。每 100 次交付耗时的中位数从 1.822 降到 1.473 ms（四轮有一轮次序相反），节省约 0.35 ms。完整 Windows 分支在两种设置下启动时，Rosetta 都在首帧前报错。[实验方法与测试](<perf/README.md>)
- **dxil-spirv + SPIRV-Cross** 从两份 fork 重新编译。c7 两个样本各跑 24 次，10 张输出每次逐字节一致，GPU 中位分别为 0.517 和 0.513 ms。已有原生回放约 0.25 ms。8bb 连跑 12 次，每次 8 张一致，另两张是已定位到并发写入的 3D 纹理。开源路径中位 136.73 ms，已有原生路径 55.13 ms。只运行 1 个工作组的反向对照成功检出 2,295,469 个差异字节。[复现说明](<shader/README.md>) · [逐轮结果](<../../evidence/forks/shader/selected.json>)

## 174 MiB 热点资源

- **Mullion** 用最新发布的 v0.5-alpha.4 和相同版本的上游测试验证文件映射的 CPU/GPU 双向读写。再按游戏捕获的两段回绕布局填入可校验的数据，直接导入与 CPU 先复制两种方法交替执行十轮，20 次 174 MiB 读回全部相同。排除首轮后的中位数为 1.626 ms 和 5.711 ms，另有一次性导入 1.551 ms。运行时需要为 GPU 保留选定的版本，直到该次 GPU 工作完成。[缓冲实验与实际边界](<memory/README.md>)
- **Xenia Edge** 用 fork 的两段原始 Metal 方法、一个测试宿主和真实 GPU 运行。预先给出变更页的情况下，直接共享和局部复制各 8 轮全量读回均正确。选定版本之后再改一个字节，直接共享会读到新值，复制仍读到旧值。游戏接入需要同时解决变更通知和版本寿命。[方法和数据](<../../evidence/forks/memory/results.json>)

## 完整游戏的额外后端对照

以 各次运行汇总 (`game-results.json`) 对照启动设置、真实加载的图形库、帧号、着色器错误和缓存记录；命令在 run_games.py。测试仅在本地独立运行目录切换图形后端。

Mullion 首轮使用图形库的默认缓存路径，归档写入失败。复测给它独立的可写目录后，成功生成 32 个管线归档，归档写入错误为零。第二轮第 2,409 帧触发了高绘制量标记，185 秒内没有抓到标记后的第一幕画面。日志反复出现部分程序要求 64 个通道、硬件提供 32 个的拒编。运行时加载的 D3D12 和 Metal 桥均来自隔离的 Mullion；这项整游戏结果与已通过的 174 MiB 传输实验可分别复现。

另外，把 Mac 上游关闭写入跟踪、完整刷新的做法移到了 Windows fork 的实验副本。两处地址写入同一内存、同帧反复改写、保留 GPU 上的局部更新和纹理刷新均做过正确性验算。源代码补丁和测试补丁单独保存，可在 Windows fork 的原始版本上应用。[兼容补丁](<../../patches/history/victory-perf/mac-full-refresh.patch>) · [回归测试补丁](<../../patches/history/victory-perf/mac-full-refresh-tests.patch>)

从本目录运行下列命令可复现额外游戏对照。需要这台机器原有的游戏文件、Wine 环境及已在实验中编译的程序。每轮有独立的缓存、存档副本和日志。

```sh
python3 run_games.py mullion
python3 run_games.py perf-on perf-off
python3 run_games.py compat-on compat-off
python3 summarize_games.py
```

### 兼容版游戏实测

同一份兼容二进制、相同 Series S 设置及按键，各观察 185 秒。原先 Windows 分支的 Rosetta 首帧错误在两轮均未再现。两项同步优化开启时保存 145 张底图，逐文件散列完全相同，解码后全黑。关闭时在第 2,119 帧触发世界绘制标记，保存到 +4，+2 到 +4 的两个截图交付间隔分别为 2.94 秒和 27.24 秒。两个运行没有相同的世界帧样本可配对，不能用它们推出同步开关对整帧的因果收益。

Windows 分支在叠加第二层之前抓取底图。我们另用 macOS 只抓该次隔离游戏的实际窗口，确认关闭同步优化的运行中出现了彩色加载插图 (`presented-125931.png`)。进入高绘制量阶段后分五个时点抽查，分别为纯黑或单色画面，最后一次窗口取样 (`presented-130113.png`)仍未显示可辨认的可玩场景。窗口未连续录像，以上以取样时刻为限。底图与叠加顺序 (`host.rs`) · 完整运行记录 (`game-results.json`)

优先保留同源 Mac 的增量刷新实验，以及 Mullion 的内存导入试验。后续若把两者接在一起，首要验收是 GPU 在游戏继续写入时还能看到本次提交选定的内容，再用实际窗口核对人物和场景能持续更新。正式启动器仍使用原配置。
