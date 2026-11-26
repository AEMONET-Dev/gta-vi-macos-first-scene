# GTA VI 第一幕在 Apple Silicon 上的渲染实验

我们在一台 M5 Max（macOS 27.3、Wine/Rosetta、GPTK 4、Series S 配置）上，从启动测试一路追到了第一幕的人脸、街景、曝光和每一帧的等待时间。本仓库公开源码、实验过程、历史补丁、未成功的尝试和逐帧测量。基于 VIctory 及其 macOS 移植，维护者为 AEMONET-Dev。本次公开快照收录截至 2026 年 11 月 25 日晚这轮整理时已记录的实验，各版本以复现指南中的提交号为准。

**第一幕能显示可辨认的人物和街景。** 在同一批先前失败的图形管线中，54 条重新编译成功。较新的图形版本修复了两组已知的片元输入不匹配，一次热缓存运行仍遇到 5 次网格输出超限。画面基准的 23 个交付间隔中位数是 2.733 秒，P95 是 4.195 秒，期间开启了诊断采集。稳定 30 FPS 仍是目标。[画面与性能提案](PROPOSAL.md)

| 想看什么 | 入口 |
| --- | --- |
| 完整过程、试过的路线、失败的对照 | [按实验顺序阅读](PROCESS.md) |
| 上游提交与讨论 | [第一幕画面和性能提案 #208](https://github.com/pragyaangaur/VIctory-macOS/issues/208) · [缓存修复 PR #205](https://github.com/pragyaangaur/VIctory-macOS/pull/205) |
| 可浏览和下载的渲染器源码 | [整合版](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-integration) · [画面实验版](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-graphics) · [性能实验版](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-performance) |
| 从原始源码还原实验、运行离线核验 | [复现指南](REPRODUCE.md) |
| 当时的记录与后续更正 | [画面](notebooks/team/Antigravity.md) · [性能](notebooks/team/Claude.md) · [整合与交叉核对](notebooks/team/Codex.md) |
| 运行记录与数据 | [逐次运行索引](evidence/run-index.csv) · [交叉对照汇总](evidence/latency/trials.json) · [各轮逐帧数据](evidence/latency/runs) |
| 作品归属、上游版本与开放许可 | [来源](SOURCES.md) · [GPL-3.0-or-later](LICENSE) |

The [English proposal](PROPOSAL.md) summarizes the first-scene rendering and frame-time findings and the next tests. Renderer source branches and offline analysis tools are available independently of game data. Game files, shaders, screen captures, raw memory and proprietary runtimes stay with the participant who supplied them.
