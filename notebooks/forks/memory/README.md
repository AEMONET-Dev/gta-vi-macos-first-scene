# 174 MiB 首幕缓冲实验

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

在 Apple M5 Max 上测试了 Mullion 的 D3D12 内存导入和 Xenia Edge 的 Metal 增量上传。实验采用第一幕抓到的资源长度和回绕位置，用确定性数据填充，能逐字节发现错误。

## Mullion

运行版本为上游发布的 v0.5-alpha.4，二进制 SHA-256 与 GitHub 发布记录一致。通过 APFS 克隆创建本实验自己的 Wine 运行时和配置。能力查询返回 0，但三种导入均返回成功。使用同版本的上游测试，核对了 4 KiB、64 KiB、192 KiB 分配上的 CPU 与 GPU 双向读写和内部地址拒绝，测试程序报告 0 项错误。

真实游戏的布局记录 (`layout.txt`) 给出了两段的顺序。前 27 MiB 从文件偏移 158 MiB 取，后 147 MiB 从文件开头取。实验按此顺序把两个区间直接从导入的内存复制到同一块 GPU 读回缓冲，并与 CPU 先拼接再提交作对照。每轮在两段各自的首尾更新内容，先后次序交替。计时含 CPU 准备、命令录制、提交与等 GPU 完成。内容核对在计时结束后进行。

10 轮共 20 次，整块读回均正确。首轮作预热，后 9 轮中位数是直接导入 **1.626 ms**，CPU 先拼接 **5.711 ms**。可复用缓冲的一次性导入耗时 **1.551 ms**。测试在 GPU 完成后才开始下一轮改写。第一幕游戏的 CPU 有并发写入，若改为游戏渲染使用，还需为 GPU 留住每次提交选定的旧版本。这个实验测量的输出是单块读回缓冲。

## Xenia Edge

驱动程序 (`run_xenia.py`) 从所 fork 的源码抽取完整的 MetalSharedMemory::Initialize 与 UploadRanges，用真实 Metal 设备运行。宿主提供游戏内存、跟踪接口和 174 MiB 资源大小。向已知的 4 KiB 合成变更页送入相邻、重叠、跨 27 MiB 边界的区间以及空列表，实际生产方法将 5 个非空范围合并成 3 个，复制和直接共享两种模式均连续 8 次通过整块 GPU 读回。

另一次受控测试在选定版本后改写一个字节。复制出来的版本仍与旧值完全相同；直接共享的版本读到了新值。该实验预先给出了变动页的编号，游戏中仍需有办法知道改了哪些页。

## 复现

在本目录运行以下命令，已归档的结果分别在 Mullion 日志 (`wrap.log`)、上游双向测试 (`upstream-heap.log`)、Xenia 日志 (`xenia.log`) 和 [数字汇总](<../../../evidence/forks/memory/results.json>)。

```sh
python3 run_mullion.py capability upstream-heap wrap
python3 run_xenia.py
python3 summarize_memory.py
```
