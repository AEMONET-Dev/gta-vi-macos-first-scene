# victory-perf 的间接调度实测

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

在这台 M5 Max 上，通过 GPTK4 的 D3D12 硬件设备运行了 victory-perf 提交 `97c3aa24263b90fa6691fca8ca88fca179593a19`。完整启动器从干净的 fork 构建，文件和摘要在 launcher.sha256。另一个本地 checkout 加入测试，并把上游测试选择的 Windows 软件适配器换成实际硬件。GPTK 在来宾中显示为 AMD Compatibility Mode。

## 100 条命令要多久

从两次第一幕运行取出五条真实的间接派发记录，x 方向分别为 0、0、1020、1020、296，y 和 z 均为 1。每批重复 20 次，走 fork 正式使用的参数准备、资源同步与 ExecuteIndirect 路径。测试用一个计数着色器逐条核对 GPU 真正执行的工作组数，再校验输入参数的全部 60 字节。100 条命令中 75 条声明使用独立缓冲，25 条请求保守的别名处理，实际分配均为独立缓冲。每组有四次独立启动，各运行十批，头两批暖机。四种设置在四轮中轮换启动顺序。

| 局部等待 | 合并等待 | 中位毫秒/批 | P95 毫秒/批 | 每批全局等待 | 每批局部等待 |
|---|---|---:|---:|---:|---:|
| 关 | 关 | 1.822 | 3.579 | 200 | 0 |
| 关 | 开 | 1.785 | 3.133 | 100 | 0 |
| 开 | 关 | 1.769 | 3.079 | 50 | 75 |
| 开 | 开 | 1.473 | 2.679 | 25 | 75 |

16,000 条派发的计数全部一致。双开相较双关在本实验的总体中位数省 0.348 毫秒（19.1%），四轮有三轮中位耗时更短。耗时从分配/录制命令列表起算至 GPU 完成，排除了进程启动和着色器编译。GPTK 返回的 GPU 时间戳差全为零，无法进一步分离 GPU 执行时间。本实验用真实游戏的派发尺寸，GPU 执行的是计数着色器，没有生成第一幕画面。[全量测量](<../../../evidence/forks/perf/results.json>) · 原始游戏输入位置 (`capture-paths.json`)

## 正确性与移植

- 在独立命令列表测试中，21 次屏障回调跨越 7 个可能发生写入的阶段。合并开关开启后实际发出 7 次、跳过 14 次。关闭时全发。这里通过显式标志模拟阶段之间的写入。开 (`groups-on.log`) · 关 (`groups-off.log`)
- 上游另外六个测试在硬件适配器上全部通过，包含计数缓冲上限、预算耗尽、未填充命令，以及 GPU 先写调度参数再直接执行的可见性。另两个测试覆盖 GPU 比较 64 KiB 区块、改后还原、一次取回实际字节与缓冲空间不足时的回退。局部等待后的 GPU 派发 (`warp_scoped_clamp_orders_preceding_uav_write.log`) · 区块差异 (`warp_card_compare_stores_the_hash_it_made_when_told.log`) · 取回与回退 (`warp_card_compare_inline_readback_and_overflow_fallback.log`)
- 系统 Wine 着色器编译器构建 fork 的参数处理程序返回 0x80004005。实验仅在私有运行目录关联本机已安装 Steam 随附的原生 x64 D3DCompiler_47.dll，就能执行同一份上游 HLSL。默认编译器对照 (`system-fxc-result.log`)
- 本地早期 Mac 管线直接就地改参数，预算使用普通读取与写入。该 fork 会写入独立临时参数缓冲，并以原子操作更新预算。移植时需要保留这两项前提和共享/别名缓冲的全局等待。[固定版本间接参数实现](https://github.com/vitutu1/victory-perf/blob/97c3aa24263b90fa6691fca8ca88fca179593a19/crates/victory-gpu/src/indirect.rs)

## 复现

从本目录执行。需要项目原有的 Windows 工具链、GPTK4 环境、五条原始记录和本机已安装的 x64 原生着色器编译 DLL。可通过 FXC_DLL 显式指定该 DLL。

```sh
python3 prepare.py
python3 build.py
python3 measure.py
python3 run.py list::tests::mac_adjacent_barrier_groups --merge 0 --fxc
python3 run.py list::tests::mac_adjacent_barrier_groups --merge 1 --fxc
python3 build.py --launcher
```

## Rosetta 完整刷新实验版

基底是 victory-perf 的 `97c3aa24263b90fa6691fca8ca88fca179593a19`。本地兼容补丁复用了 VIctory-macOS 的 Darwin/Wine 检测和跟踪开关，来源提交为 `c78cfdc462696d448c4b5b2271e653ea3dff41c6`。Windows 分支继续负责资源翻译、绘制和提交。移植的代码放在资源管理层，游戏文件保持原样。

- [兼容补丁](<../../../patches/history/victory-perf/mac-full-refresh.patch>) 关闭运行时分配的写入跟踪，把缓冲区和纹理送回完整读取路径，未跟踪的动态缓冲按每个命令列表重新准备。同一帧内的后续提交由此可以读到更新。GPU 正在写的缓冲区域仍由原分支负责合并。动态缓冲原先的跨列表节流省掉过大量工作，这个正确性优先的设置可能增加整帧用时。
- [独立回归测试补丁](<../../../patches/history/victory-perf/mac-full-refresh-tests.patch>) 应用在上述补丁之后。测试中，同一区域的两份别名映射多次改写，包含 64 KiB 边界，6 次完整刷新逐字节一致；第 7 次只保留 GPU 正在写的 4 字节，旁边的 CPU 写入及时可见。纹理在同一帧连续换成第二套内容、再清零，三次实际 staging 输出（含行间填充）均与软件参考相同。缓冲测试 (`compat-tests.log`) · 纹理测试 (`compat-texture.log`)
- 实际交给整游戏对照的 x86-64 release diagnostics 启动器 SHA-256 为 `4006a16b64f61045006965b35f147ee21477ad884cb4f17c2c276e3a51925b47`。在 A/B 两边显式设置 `VICTORY_FAULT_TRACKING=0`；只有两项屏障设置不同。二进制 (`victory.exe`) · 完整摘要 (`compat-launcher.sha256`)

在上述基底的独立 checkout 中依次执行 `git apply /path/to/mac-full-refresh.patch` 和 `git apply /path/to/mac-full-refresh-tests.patch`。在当前项目中，`python3 build.py --compat --launcher` 构建完整程序，`python3 build.py --compat` 构建 GPU 测试，`python3 build.py --compat --hle` 构建运行时分配测试。只有本地实验副本应用了改动。

运行时分配测试也已通过。实际调用 XMemVirtualAlloc 得到的常规内存符合原有的对齐、访问和地址范围要求，在关闭跟踪时未设置写入跟踪属性。分配测试 (`compat-allocation.log`)

Windows fork 自带的 PNG 截帧取自呈现底图。它先把底图复制到交换链，同步安排从原底图读回，再把第二层画到交换链。此次开启屏障的整游戏记录确实进入了第二层合成代码。因此全黑 PNG 只证明采到的底图为黑，最终窗口及游戏世界的有效画面没有由这批 PNG 验证。[底图读回与其后的合成](https://github.com/vitutu1/victory-perf/blob/97c3aa24263b90fa6691fca8ca88fca179593a19/crates/victory-gpu/src/host.rs#L6619-L6657)

同一个兼容版各运行 185 秒。开启两项同步的 145 张底图均为全黑。关闭两项同步后，第 2119 帧首次达到 500 次绘制，139 张底图里有 4 种 PNG；除全零外，2121–2123 帧所有像素的 RGB 通道只取 0 或 1（满值 255），画面无法辨认。这两个进程的游戏负载不同，无法给出世界场景的整帧提速百分比。同步开启日志 (`game.log`) · 同步关闭日志 (`game.log`)
