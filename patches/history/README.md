# 历史补丁

这份目录保存试验时实际用过或对照过的补丁，包括后来被替代的实现。部分从上游移植，文件内容保留原来的路径、顺序和基线假设。[来源](../../SOURCES.md)

| 目录 | 内容 |
| --- | --- |
| [early-renderer](early-renderer) | 早期整帧渲染候选，含快照与写回 |
| [exploration](exploration) | 上传池、稀疏内存、纹理读取、材质和着色器、上游修复、写入跟踪及候选对照 |
| [latency](latency) | 两代交叉试验的计时、快照合并与部分驻留 mip 读取 |
| [victory-perf](victory-perf) | Windows fork 的全量刷新兼容移植及配套测试 |
| [graphics-history](graphics-history) | Wave32、网格打包、系统值修复、导出顺序、Wave64 和阶段合并补丁 |
| [branches](branches) | 定点写入跟踪、实验缓存键和批次计时的单项差异 |

需要完整可用的版本时，从[源码与复现入口](../../REPRODUCE.md)选择对应版本。历史补丁之间存在互斥方案，不能作为一条连续序列统一套用。
