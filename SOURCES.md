# 来源与许可

AEMONET-Dev 以 **GPL-3.0-or-later** 发布本项目自己的实验代码、测量与研究笔记，沿用 VIctory 工作区声明的许可证。保留原项目的署名和许可证。第三方库和独立项目按各自的许可证使用。[许可证全文](LICENSE) · [VIctory-macOS](https://github.com/pragyaangaur/VIctory-macOS) · [VIctory](https://github.com/Marvisak/VIctory)

- **渲染器源码**建立在 `pragyaangaur/VIctory-macOS@cc8e90e` 上。七份源码分支展示独立实验的可构建内容。公开快照的 Git 提交号与实验室提交号不同，原始编号和补丁校验值保存在[版本清单](patches/profiles/manifest.json)。
- **引用或移植的图形工作**包括上游的网格导出顺序、配对的网格与像素着色器、单波 LDS 及 Wave64 翻译。历史补丁夹也收录研究时对照过的上游差异。具体贡献归各原作者。[导出顺序](https://github.com/pragyaangaur/VIctory-macOS/pull/167) · [配对链接](https://github.com/pragyaangaur/VIctory-macOS/pull/161) · [单波 LDS](https://github.com/pragyaangaur/VIctory-macOS/pull/148) · [Wave64](https://github.com/pragyaangaur/VIctory-macOS/pull/180)
- **性能和后端对照**使用 VIctory-macOS、victory-perf、Mullion、dxil-spirv、SPIRV-Cross、Xenia Edge。保留了 fork、上游作者、实验提交和结果。Xenia 对照脚本从使用者自己的相应开源版本取实验方法，Mullion 的发布版测试另记版本。XCL 在最初阶段单独做了启动和呈现对照。[六个项目的固定版本](evidence/forks/forks.json) · [测量与实际边界](notebooks/forks/README.md)
- **素材和运行时**由参与人本地提供。公开材料采用本组的源码、合成测试、计数、时间与内容指纹。游戏程序、游戏数据、原始着色器、反编译出的游戏着色器、游戏画面、原始内存及 Apple/CrossOver 运行时不随仓库分发。[上游贡献约定](https://github.com/pragyaangaur/VIctory-macOS/blob/cc8e90e7e32f803fcb1873b3b921709f08a51829/CONTRIBUTING.md)
