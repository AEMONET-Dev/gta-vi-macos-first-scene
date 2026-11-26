# 首幕缓冲区对照程序

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

Windows release 的正式画面基线是 `b45492b5709cd58a4d7bf0d0693e628c514ee4b2dfd8efad9b82ba3bb5633d2d`。本试验程序为 `candidate.exe`，SHA-256 为 `da9e3f0bcbdc079165b1f5aec1d1efbdddfbb1cf42888547e1f30ce51f095d2a`。`b454.patch` 包含相对该源码的八个文件修改，已用 `git apply --check` 验证。两组共用正式材质与光照设置、同一着色器服务、同一份驻留 mip 修复。

`VICTORY_LATENCY_MODE=abba` 或 `baab` 对照一次运行中的连续世界帧。前八帧沿用原始读法，自世界帧偏移 8 开始，每段两个世界帧，A 为原始读法，B 为独立映射逐字节比对。只对整块 174 MiB 缓冲区生效。使用 `on` 或 `off` 可以固定读法并记录时间。未设置模式时不输出时间线。`VICTORY_MERGE_WRITE_SNAPSHOTS` 默认关闭，且不会随世界帧变化。上传任务保持原始顺序。

`174MiB refresh` 行中的 `enabled` 是该次所选路径。`attempted` 表示实际调用独立映射函数，`success` 表示该次成功，否则执行原始受检读法。成功时 `stored` 是改写的字节数，默认或回退时是完整完成刷新范围的字节数。中途丢弃的独立比对可能已经写过部分目标，字段不包含这部分额外写流量。`elapsed_us` 是从本次刷新开始到完成的总微秒数，包含回退。`begin`、`end` 和其他时间线条目共用单调时钟，`frame` 是该次读操作开始时的已提交显示帧，`world` 是相对首幕的偏移。各工作线程的耗时不能相加当作帧耗时。

针对别名修改、取消或改变权限、映射中途改变、快照空隙、原始回写范围和跨提交更新的 Windows 回归日志位于上一级目录。缓冲映射只保存读取位置，每次均重新验证可见性并比较内容。实机结果另见运行目录。
