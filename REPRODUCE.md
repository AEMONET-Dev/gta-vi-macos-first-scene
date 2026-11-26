# 复现和核验

## 获取完整源码

克隆本仓库后，以下命令在新目录准备一套独立渲染器。脚本从固定的公开提交取源码，核对补丁 SHA-256，然后应用所选版本。需要 Python 3 和 Git。

```sh
git clone https://github.com/AEMONET-Dev/gta-vi-macos-first-scene.git
cd gta-vi-macos-first-scene
python3 tools/prepare_renderer.py integration ../first-scene-renderer
cd ../first-scene-renderer
cargo test --release -p victory-shader --lib
```

也可以直接下载可浏览的源码分支。源码分支保留了上游许可证、原版说明和构建工具。还原脚本会保留固定上游版本的 CI 文件。对研究过程中使用的每个版本，我们逐文件核对了还原后的内容，与本地研究版本的差异仅为一份记录本机目录的交接文档。

| 参数 | 本地研究提交 | 包含的改动 |
| --- | --- | --- |
| [baseline](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-baseline) | `b229d13` | 有人物和街景的 picture-textures 对照版本 |
| [graphics-v3](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-graphics-v3) | `8e1b338` | Wave32、网格导出顺序、通用输出行打包和系统值修正 |
| [graphics-halves](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-graphics-halves) | `d253f54` | Wave64 半波实验 |
| [graphics](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-graphics) | `e2c4a8b` | 增加单波共享内存同步，另做 PC FSR 开关对照 |
| [performance-timing](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-performance-timing) | `2ba7394` | 单缓冲写入跟踪、漏写审计和批次计时 |
| [performance](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-performance) | `8bb04e4` | 增加分线程复制统计和堆同步锁的移植 |
| [integration](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-integration) | `a6aeaa6` | 画面 v3、写入跟踪、批次计时及缓存修复 |

固定上游基线为 `cc8e90e7e32f803fcb1873b3b921709f08a51829`。完整研究版本号和补丁校验值见[版本清单](patches/profiles/manifest.json)。Windows 渲染器和原生 Mac 翻译服务需要相配的构建。构建目标、Rust 工具链和 Wine 设置见[对应版本的上游指南](https://github.com/pragyaangaur/VIctory-macOS/blob/cc8e90e7e32f803fcb1873b3b921709f08a51829/docs/macos.md)。

## 复算测量

从本仓库主目录运行以下两项离线检查。测试使用合成的时间线和已知答案，在没有游戏文件的环境里也能运行。

```sh
python3 -B -m unittest discover -s tools -p 'test_render_latency.py'
python3 -B tools/test_refresh_frames.py
```

将自己的实验日志交给分析器，可以复算某次交叉实验的帧耗时和各阶段用时。下例中的日志需要包含 `frame delivered`、`submission timeline` 和世界场景标记。模式和顺序必须与采集时相同。

```sh
python3 tools/render_latency.py /path/to/your-run.log --subject xmem --mode baab -o timings.json
```

每次局部计时应标明测的是单次调用、累积工作线程时间还是交付两帧之间的墙钟时间。存档包含[四轮逐帧观测](evidence/latency/runs)、[低采集轮次的汇总](evidence/latency/trials.json)及[早期三版的逐帧对照](evidence/early-submit/frame-times-final.json)。[运行索引](evidence/run-index.csv)记录采集时各日志能找到的世界标记、保存帧号和错误消息行数。空标记表示该日志没有相应观测，缓存和编译覆盖也会改变错误行数。

## 实机重放所需的本地输入

[实验脚本](lab)保留了最初的渲染配置、按键、资源版本与采集开关。它们使用工作站的目录布局。游戏副本、Wine bottle、Apple 运行时、Windows 依赖、已构建程序，以及回放用的帧或着色器，由复测者在本机准备。

原始布局在实验根目录下使用 `game/`、`bottles/`、`runtime-4/`、`tmp/`、`logs/` 与 `victory-gptk4-run/picture-textures/`。最后一个目录存放成对构建的 `victory.exe` 和 `victory-metal-daemon`。Wine bottle 的 C 盘映射需要与 [run_test.py](lab/run_test.py) 使用的游戏、缓存、捕获和渲染器路径一致。图形实验记录各自的渲染器和服务端指纹。专项脚本的输入见各实验笔记。

同一台 Mac 上一次运行一个游戏测量，先固定配置、预热状态和帧窗口，再分别测基准版、单项改动和整合版。每轮保存绘制量、P50 和 P95，并用同镜头检查人物、材质和光照。早期记录里的命令与临时文件名供追溯原实验使用。
