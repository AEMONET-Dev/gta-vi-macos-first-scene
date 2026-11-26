# 开源着色器在 M5 Max 上的第一幕回放

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

用同一套翻译和执行设置独立重放第一幕的三份计算输入。每轮都重新建 Metal 资源、载入游戏原始数据，执行后从 GPU 取回并逐字节比较十张输出，共 3,438,080 字节。机器为 Apple M5 Max。

| 着色器及输入 | 验证轮数 | 每轮完全相同的输出 | GPU 中位耗时 | P95 |
|---|---:|---:|---:|---:|
| c7feccae4e01e454，第一份，1,020 组 | 24 | 10/10 | 0.517 ms | 0.533 ms |
| c7feccae4e01e454，第二份，1,020 组 | 24 | 10/10 | 0.513 ms | 0.527 ms |
| 8bbcbda6008eaa37，296 组 | 12 | 8/10 | 136.729 ms | 146.414 ms |

c7 可以用作这两份输入的开源回放参考。8bb 每轮都仅在 d88 和 d96 两张三维纹理有差异，37,719 至 39,741 个字节。这两张纹理在旧路径分别派发奇数组和偶数组时已观测到多个写入者，尚未证明所有差异均出于该原因。上一版原生回放的 c7 约 0.25 ms，8bb 约 55 ms；本实验的 GPU 时间只含单次计算指令缓冲。只运行 c7 的 1/1,020 组时，负对照如期检出 2,295,469 个差异字节。最终数据和完整哈希见 [selected.json](<../../../evidence/forks/shader/selected.json>)、负对照 (`selected-negative.json`) 和 复现日志 (`reproduced.log`)。

## 找到的有效设置

使用 dxil-spirv 的死代码消除（`--dead-code-eliminate`），SPIRV-Cross 输出 MSL 3.2。Metal 宿主为每个捕获的纹理描述符建立对应的 1D、2D、2D 数组或 3D 资源。采用 Metal 安全浮点算术和快速数学函数（`MTLMathModeSafe`、`MTLMathFloatingPointFunctionsFast`）。保持每份输入原来的工作组数量、资源内容和采样器。两套开源翻译器都从 fork 的源代码构建；最后编译和派发使用系统 Metal 框架。

- 默认翻译时 c7 两份输入都只有 9/10。原先有差异的 d120 以相邻半精度数为主。
- 仅换安全浮点曾令 c7 两份输入达到 10/10，但独立重跑时仍有整组漏写。已在新建的原生对照中重复 20 次，20 次均相同。
- 加死代码消除、保持旧纹理绑定后，c7 两份输入分别 50/50 次通过，8bb 仍只有 4/10。对 8bb 补全纹理类型、分别对照三种数学模式与两种函数精度后，最佳设置达到 8/10。详见 复跑 (`confirmed.json`)、纹理形状对照 (`shapes-and-control.json`)、六种数学配置 (`modes.json`)。
- 同一套最终设置在独立两批回放中重复。表中给出从原始 DXIL 重新翻译、重新构建宿主后的后一批。前一批输出 (`final_replay.log`)

## 复现

本目录保留构建好的命令行工具、宿主源文件及脚本。保留项目中现有游戏归档和清单，在本目录运行以下命令即可从原始 DXIL 重新翻译并运行所有最终对照。需要带 NumPy 的 Python 和可调用的 Xcode 命令行工具。

```sh
python3 reproduce.py
```

如需要重建翻译工具，使用 CMake、Ninja 和 C++ 编译器，在本目录执行以下命令。dxil-src 来自 fork 的 694cc674380a47767b56d63b9dec196aec963115，子模块的提交已按该源树核实。单独使用的 SPIRV-Cross fork 为 57e4efcc84112abcd426dc5ec248b5339bee04f2。它和 dxil-spirv 内置的 be71ee8c12cd7dc5ca8fa9581f708c2e8561fe2a 版本分别有独立构建位置。

```sh
cmake -S dxil-src -B build-dxil -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS_RELEASE='-O1 -DNDEBUG'
cmake --build build-dxil --target dxil-spirv spirv-val -j 6
cmake -S ../repos/SPIRV-Cross -B build-cross -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS_RELEASE='-O1 -DNDEBUG' -DSPIRV_CROSS_ENABLE_TESTS=OFF
cmake --build build-cross --target spirv-cross -j 4
python3 reproduce.py
```

## Mullion 实际游戏的波宽错误

第 2,545 帧日志中出现了 c7 和 8bb 的首次绑定，后续没有这两个哈希的管线创建成功或失败记录。启动阶段要求 64 个并行通道、硬件只有 32 个的错误旁，运行时明确列出拒编的三个其他计算程序：13dfdd3b1136e407、88994c3496045f51、b68f0931f1f47e61。Mullion 在 DXIL 入口有显式 WaveSize（属性 11）且与 GPU 波宽不符时发出这条错误。离线归档的 c7 和 8bb 入口只有属性 0 与 4，工作组为 8×4×1；本次游戏缓存未找到和这两份归档 DXIL 逐字节相同的目标。因此离线结果不能判定本次 Mullion 实际为这两道建立管线的结局。离线宿主专门绑定这两道计算的资源、采样器、寄存器；游戏仍需为实际被拒的程序正确实现 64 通道的跨通道操作，并覆盖其他绘制、资源及呈现流程。游戏日志 (`game.log`) · Mullion 波宽检查 (`dxil_converter.cpp`) · c7 的 DXIL (`c7feccae4e01e454.ll`) · 8bb 的 DXIL (`8bbcbda6008eaa37.ll`)
