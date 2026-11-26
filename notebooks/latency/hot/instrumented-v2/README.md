# 首幕交叉对照与等待时间

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

`candidate.exe` 的 SHA-256 为 `855cbe3bc4f7b7315dc073c9b840718227fbf47d82c0ea304fe84e9ab997626c`，与 v1 共用正式光照、材质、驻留纹理修复和原始上传顺序。`b454.patch` 可直接套用正式源码，`from-v1.patch` 可套用第一轮实验源码。两份补丁均通过 `git apply --check`。

**对照参数。** `VICTORY_LATENCY_MODE=abba` 或 `baab` 每两帧一段，在八帧预热后交替 A（关闭）、B（开启）。`on` 和 `off` 固定策略。没有模式时不写时间线。默认比较 174 MiB XMem 缓冲区读法，保持快照策略恒定。`VICTORY_LATENCY_SUBJECT=snapshots` 选择对照同一提交内的快照合并，并全程关闭 XMem 快速读取。

**fence timeline。** 时间点采用与 `frame delivered` 相同的单调微秒时钟。每条对应一个 D3D12 队列提交序号 `sequence`、组号 `group` 和命令列表数 `lists`。

| 相减 | 对应区间 |
|---|---|
| submitted − started | ExecuteCommandLists 调用 |
| signaled − submitted | 队列 Signal 调用 |
| armed − signaled | SetEventOnCompletion 调用 |
| returned − armed | 等待事件至 WaitForSingleObject 返回 |

`waited=false` 表示 Signal 或事件注册失败，本次没有调用事件等待。`waited=true` 表示进入了原有等待路径。最后一段是 CPU 等待事件的墙钟时间，可能与此前就已开始的 GPU 执行重叠。

**snapshot timeline。** `declarations` 是排序或合并前的写入声明数，`merge` 记录实际策略。`elapsed_us` 涵盖该提交中快照的排序、登记和取样，排除之前的缓冲传输。`174MiB refresh` 保留 v1 的逐次记录。
