# Metal 编译服务与 GTA 首场景

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

2026-11-25 三次本机运行里，Metal 编译服务都在入场后的实际画面阶段反复崩溃。取 macOS `launchd` 的 `exited due to SIGSEGV` 记录，以日志里的 Wine 客户 PID 与每次启动的编译服务 PID 配对，每个编译服务进程只计一次。窗口为纽约时间 08:06:12–08:18:36。游戏日志的 UTC 时间已转换为纽约时间。

| 运行 | 客户 PID | 首场景帧 | 编译服务崩溃 | 入场后 | 入场 30 秒后 | 最晚一次距入场 | D3DMetal 标为无操作的不同图形管线 | 报错的 Compute 编译次数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 正式光影版 08:06:12 | 38339 | 1609 | 98 | 92 | 40 | 93.903 秒 | 84 | 143 |
| 只改纹理版 08:09:44 | 39527 | 1726 | 71 | 65 | 21 | 89.403 秒 | 74 | 141 |
| 调度实验 08:15:14 | 41058 | 1683 | 79 | 71 | 29 | 108.079 秒 | 73 | 145 |

三轮共 248 个不同的编译服务进程收到了段错误退出信号。正式版采用互斥启动脚本，Wine 于 08:06:12.832 以 PID 38339 登记，游戏日志于 08:06:12.901 开始；这个客户的系统记录一直跟随该场运行。纹理版有直接的系统责任进程证据。现存 25 份 `.ips` 均写明 `responsibleProc=victory.exe` 和 `responsiblePid=39527`，捕获时间 08:11:05.2210–08:11:18.0944，同为 `EXC_BAD_ACCESS/SIGSEGV`，崩溃线程首个指令都在 `libLLVM.dylib + 11510860`。例如 `~/Library/Logs/DiagnosticReports/MTLCompilerService-2026-11-25-081107.000.ips` 对应服务 PID 40016，`MTLCompilerService-2026-11-25-081118.0002.ips` 对应 PID 40118。`launchd` 中 40016 的客户为 39527；服务在 08:11:05.170 退出，同一时间客户系统日志可见编译连接中断。调度版的三次窗口快照均记录所有者 PID 41058。启动脚本输出的是包装进程 PID，与这三个实际客户 PID 分别记录。

三轮的 D3DMetal 私有系统日志都有两类错误记录。一类为 `MetalIRConverter: UnsupportedWaveSize ... Must be 32` 及 `Failed to compile stage Compute - error:6`，另一类明确写着 `Failed to create pipeline state ... fs=<private> ... marking PSO(...) as no-op`。后者表明原本依赖这条管线的绘制可能没有任何输出，且每轮都有十几个以上的无操作报告出现在入场 30 秒以后。系统隐去对应着色器名，报错里也没有供游戏日志关联的材质、灯光通道或资源号。记录不支持将每一次编译服务崩溃归因于 wave 大小，亦不足以认定某一条管线就负责画面中的蓝斑或阴影。

三次 ARM64 着色器翻译服务都正常结束，且 `shader_refusals=0` 和 `transport_errors=0`。游戏三次都有 29 条 FSR4 预编译格式回退至 `NewBE` 的信息，也都记录当前深度、材质光照已写入、升采样取得当前视图。系统后端仍可能在完成前端翻译后拒绝管线。截图显示相同蓝白异常进入两次正式/纹理运行的照明输入，足以把排查范围定位至最终显示之前。现有资料没有把具体 Metal 错误和这张中间图逐项关联。编译服务报错随着场景继续发生，其中许多并行；这些计数无法换算成整帧阻塞时间。

逐次统计与全部 IPS 文件名 (`compiler-events.json`) · launchd 退出记录 (`compiler-signals-key.log`) · D3DMetal 原始报错 (`compiler-driver-key.log`) · 编译连接中断 (`compiler-xpc.log`) · 游戏截图对照 (`scene-pairs.jpg`)。运行 `python3 compiler_events.py` 可从上述文件复算；本机 IPS 会由 macOS 轮换，JSON 保存了本次实际读取的文件名和责任进程字段。

## 正式版第 +17 至 +40 帧

用 7 张实际截图帧号作锚点，以每次呈现的 `title memory this frame` 时间划分前后两帧的间隔，逐段检查相邻截图间记录数与帧数相符。23 个相邻呈现间隔逐项吻合 [现有帧时结果](<../../../evidence/latency/trace/results.json>)。将时间戳落在同一间隔 `(起点, 终点]` 内的编译服务段错误按服务 PID 归入该帧。

| 这个间隔是否遇到编译服务退出 | 帧数 | 服务退出次数 | 帧时中位数 | 最快至最慢 |
| --- | ---: | ---: | ---: | ---: |
| 是 | 6 | 36 | 2411.676 ms | 1399.113–2733.454 ms |
| 否 | 17 | 0 | 2933.373 ms | 2066.339–5367.210 ms |

这 17 帧全部超过两秒，说明即使当帧没有新的 Metal 崩溃，也有足以造成秒级延迟的工作。分组受到场景、绘制量和编译并行影响；同时发生的事件无法给出编译服务对单帧的因果耗时。已经失效的图形管线也可能继续影响后续画面。[逐帧时间、服务 PID 与分组](<../../../evidence/latency/texture/compiler-frame-overlap.json>)
