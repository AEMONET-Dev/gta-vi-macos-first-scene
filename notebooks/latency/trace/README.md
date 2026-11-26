# 第一幕实际画面的延迟

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

取数使用对话《核验 Antigravity 新方案》的 1080p 材质合成版本。它读回送往交换链的合成结果，能看到人物、车辆和街道。还有光照偏色、暗区和画面切换。四个开发版本分别保留自己的运行，正式版另外记录；跨版本镜头和每帧绘制量均会变化。

数据和脚本均在本目录。运行 `python3 analyze.py` 会从工程已有日志重新生成 [results.json](<../../../evidence/latency/trace/results.json>)。可用 `--extra NAME=/path/to/run.log --output /path/to/file.json` 对相同仪表的新游戏运行做同口径测量。脚本不启动游戏。原始二进制散列、截图路径、各次采样及复制热点都在 JSON 内。

## 当前已安装的画面合成版

本次正式版 SHA-256 为 `b45492b5709cd58a4d7bf0d0693e628c514ee4b2dfd8efad9b82ba3bb5633d2d`。另一个对话从实际入口运行 211 秒，日志确认世界起始帧为 1609，有当前场景的深度和材质输入，带清光和合成设置。第一幕 +17 至 +40 的 **23 个实际交付间隔**，平均 **2937 ms**、中位 **2733 ms**、P95 **4195 ms**。其中 11 个宿主统计样本的提交前准备中位 **1690 ms**，每个样本的准备占比中位 **72%**；提交并等 GPU 栅栏完成 **446 ms**，回写 **144 ms**。各项中位来自各自样本，不能作为某一帧的完整加和。正式版运行核验 (`final-play-check.log`) · 正式版入世界日志 (`victory-run-picture-20261125-080612-kz1g1gwi`) · 相对 +26 帧 (`frame-1635.png`)

在 10 个完整复制统计窗口中，每帧的缓冲批量刷新中位 **1276 ms**，约有 **20,293 次范围刷新、18.77 GB**。单独的 **174 MiB** 资源平均每帧刷新次数在各窗口的中位为 **24 次**，累计 **4.38 GB、149 ms**。提交前收集并更新纹理堆的中位时间为 **535 ms**。纹理检查量中位 **9.24 GB/帧**，需重新处理的纹理范围中位 **21.15 MB/帧**。这些计量分别覆盖多次提交或刷新任务，多个小计相互包含或跨工作线程重叠。游戏在窗口外仍可能有帧级抖动。

## 计时边界

每次 D3D12 呈现返回后，程序在 Meter::frame 中记录当前帧用到的游戏内存、绘制和计算。空帧可能省略这一行。脚本先用两张已保存的合成截图确定绝对帧号，再校验两截图间这个标志的个数等于帧号差。通过校验才给出逐帧耗时、平均值和 P95。该值包括本次测试开启的截帧和诊断开销，时钟位于应用端，不能测出按键到显示器发光的延迟。呈现与记录代码 (`host.rs`) · 逐帧内存标志 (`memory.rs`)

提交线程持有提交锁后开始准备，先更新描述符，收集与检查可能被采样的纹理，再运行各列表的待处理刷新、复制和快照。准备完成后调用 ExecuteCommandLists，随后等栅栏完成。宿主计数里的“on the GPU”含提交、驱动往返和等待；回写另计。准备线程等待提交锁的时间没有归入准备项。宿主耗时计量每次统计至少 1 秒，并且每约 5 秒只打印最近的一次统计；分项为这些已打印样本的中位，不能当作每一帧均有同样耗时。提交与栅栏 (`host.rs`)

复制计数器累积至输出约五秒一次，除以这段实际呈现的帧数。脚本只选首尾都落在目标场景窗口里的完整计数器区间。刷新批次的时间含受检读和工作线程汇合；各工作线程的逐资源计时可以重叠。纹理表准备也含检查哪些资源需传输的时间。这些计数不能和上层宿主准备时间相加。计数器 (`copystat.rs`)

## 开发版本同一相对窗口

每轮以自身世界帧阈值为原点，取第 +14 至 +26 帧的 12 个真实呈现间隔。下表按每轮实际采样列示。镜头和工作量不固定，组间差值不能单独归因于代码。

| 版本 | 12 帧平均 / 中位 | 宿主准备样本中位 | 提交到完成样本中位 | 绘制数样本中位 |
|---|---:|---:|---:|---:|
| 初始材质设置 | 3939 / 3650 ms | 2619 ms | 524 ms | 3870.5 |
| 仅表项实验 | 5157 / 4958 ms | 4917 ms | 516 ms | 3421.0 |
| 恢复驻留材质 | 2851 / 2800 ms | 2316 ms | 429 ms | 3264.0 |
| 清理光斑实验 | 2314 / 2241 ms | 1332 ms | 478 ms | 3615.5 |

恢复材质版在 +14 至 +40 的 26 个间隔中，中位 2685 ms，P95 4949 ms。提交准备占宿主样本的中位 77.7%。清理光斑版在 +14 至 +48 的 34 个间隔中，中位 2741 ms，P95 4267 ms；准备占比中位 72.1%。这两轮同一机位的路面标线相似度由画面线程另行核过。同镜头记录 (`shot-comparison.json`)

## 卡在哪一层

1. **CPU 等待游戏数据刷新完成。** 清理光斑试验的 14 个完整复制窗口，刷新批次中位 1085 ms，每帧约 1.94 万次范围刷新。没有可靠写入追踪时，Shadow::update_free 会通过 copy_mapped 再次读入整个请求区间。主资源是 174 MiB；每帧中位刷新 23.35 次，相当于累计 4.26 GB，单项耗时约 145 ms。只解决这一块仍有大量其他缓冲和纹理要处理。范围规划和刷新批次 (`host.rs`) · 数据更新与完整回退 (`shadow.rs`)
2. **CPU 检查纹理是否需要上传。** 同一批窗口，纹理堆准备中位 787 ms。平均每帧的检查数据量在各窗口的中位为 9.56 GB，需重新处理的纹理范围合计中位为 18.2 MB。后一个数字统计发生变化的整个纹理或驻留区间长度，实际被游戏改写的字节数可能更少。程序先按字节比较，需要时才重排纹理。因此减少检查量必须同时保证游戏写入、别名映射和流式加载的新材质及时可见。纹理检查与重排 (`texture.rs`)
3. **提交后等待和回写也影响预算。** 清理光斑试验的 15 个宿主样本，提交到完成中位 489 ms，回写中位 159 ms，即使单独看都高于 30 FPS 的 33.3 ms。计时不能分解出 GPU 各着色器的准确执行时间，也不足以确定某道灯光处理程序造成了全部等待。

## 新版逐帧日志

[render_latency.py](<../../../tools/render_latency.py>) 读取带有 `render_latency` 事件的实机日志。以下示例从 Mac_GTA 工作区目录运行。运行目录里的 `run.json` 记有正式日志和试验模式，也可以直接指定游戏日志并传入 `--mode abba`、`baab`、`on` 或 `off`。

```sh
python3 GTA-VI-launch-test/first-scene-latency/trace/render_latency.py GTA-VI-launch-test/first-scene-latency/runs/schedule-ornp07qk -o /tmp/gta-schedule.json
python3 GTA-VI-launch-test/first-scene-latency/trace/test_render_latency.py
```

- `frames` 保留世界第一帧起的所有相邻呈现区间，每项附起止时间、绘制与派发增量。缺失中间呈现的区间会写入 `skipped_anchor_intervals`。第一个呈现点之前缺少完整帧起点。交付时间戳取自 DXGI Present 调用返回后，日志没有记录显示器真正点亮像素的时间。
- `exclusive_ms` 把各提交的时间段裁到两次实际交付之间，同一时刻只归入一个类别，加总精确等于帧间隔。刷新批次单列于 `deferred_buffer_batch`。只等队列且没有其他已记录提交在执行的时间归入 `queue_wait_only`。其余重叠的等锁时间见 `nested_ms.queue_wait_while_active`。提交及栅栏等待含驱动开销；日志没有纯 GPU 执行时间。`outside_recorded_submission` 涵盖提交打点之外的所有工作和空档。
- `by_mode` 默认从区间起点为世界第 +8 帧起统计，也可传 `--min-offset 0` 查看全部。排序试验有实际 `schedule` 标志；缓冲读取试验有批次的 `xmem` 和每次读取的 `enabled` 标志。跨模式的区间标为 `mixed`，缺少观测标为 `unknown`。`planned_start`、`planned_end` 对应该区间两端的 ABBA / BAAB 计划。每个区组含 8 个区间，每种状态连续两帧切换。
- `by_mode_without_png_saves` 剔除两次交付之间日志记有任意 PNG 落盘的帧。部分材质 PNG 在对应画面交付后才保存。该筛选仅使用已记录的保存事件，无法消除诊断工具的全部影响。各组同时报告工作量，P95 采用最近秩法。
- `hot_events` 保留每次 174 MiB 读取的原始打点。`hot_calls_by_mode` 对筛选时段内开始的读取，按实际开关统计尝试、成功、写入字节、每次耗时。跨帧读取的次数和字节归入它开始的帧，时间按实际重叠拆到各帧。`hot_worker_ms` 汇总各工作线程的耗时，`nested_ms.hot_copy_union` 给出这些读取覆盖的墙钟时间，两者均与缓冲批次时间有重叠。

- v2 的 `fence timeline` 进一步给出执行提交调用、发送信号、登记事件、等待返回的墙钟时间，分别见 `nested_ms.fence_execute_call`、`fence_signal_call`、`fence_arm_event_call`、`fence_wait_return`。未成功登记事件的返回耗时见 `fence_without_wait_return`。各项均已包含在对应独占阶段中。每帧给出 fence 调用数和未进行等待的数量。
- v2 的 `nested_ms.snapshot_union` 是快照回调的时间并集，`snapshot_with_declarations` 只含有实际声明的回调。实验主题从 `run.json.settings.VICTORY_LATENCY_SUBJECT` 读取；直接使用 v2 快照日志时加 `--subject snapshots`。`by_mode` 使用快照打点的 `merge` 值，包含零声明打点；若开关在交付区间内变化则归入 `mixed`。没有快照打点则为 `unknown`。`by_applied_snapshot_mode` 只使用有实际声明的打点，零声明和没有打点的帧归入 `no_declarations`。可用每帧的 `snapshot_modes_intersected` 区分两者。顶部 `v2_event_counts` 给出该日志实际支持的 v2 记录总数。
