from pathlib import Path
from statistics import median
import re
base=Path.cwd();logs=base/'logs'
def link(label,p,line=None):
 p=Path(p);p=p if p.is_absolute() else base/p
 dest=str(p)+(f':{line}' if line else '')
 return f'[{label}](<{dest}>)'
def loc(s,key):return 1+next(i for i,l in enumerate(s.splitlines()) if key in l)
native=logs/'victory-run-gptk4-native-exposure-indirect-ab.log';native_text=native.read_text()
release=logs/'victory-run-native-spd-20261125-034721.log';s=release.read_text()
frames=re.findall(r'frame saved path=.*?\\(frame-(\d+)\.png)',s)
frames=[(name,int(n)) for name,n in frames if n not in ['1','2','4','8','16','32','64','128','256','512','1024','2048','4096']]
assert frames and 'exposure immediately after dispatch' in s
real=base/'victory-frames'/release.name.removeprefix('victory-run-').removesuffix('.log')
pairs=re.findall(r'exposure immediately after dispatch values=\[([-+\d.eE]+), ([-+\d.eE]+)\]',s)
assert pairs
times=[]
start=loc(s,'exposure immediately after dispatch')
for l in s.splitlines()[start-1:]:
 m=re.search(r'([\d.]+) ms/frame .*?copies ([\d.]+) ms ([\d.]+) MB.*?; ([\d.]+) on the GPU',l)
 if m and '(0 being made)' in l:times.append(tuple(map(float,m.groups())))
stats=''
if times:
 t,c,b,g=map(median,zip(*times));stats=f'管线待编译数为零且已观察到曝光输出的 {len(times)} 条帧率记录中，帧耗时中位数为 {t/1000:.2f} 秒，CPU 拷贝中位数为 {c/1000:.2f} 秒、{b/1000:.1f} GB。每行是 VIctory 自己的一段帧率统计。'
a=logs/'victory-run-gptk4-spd-store-pass-ab.log';b=logs/'victory-run-plane0-scene-20261125-031446.log';instr=logs/'victory-run-gptk4-time-spd-probes.log'
primary=link('第一张发布版主画面截帧',real/frames[0][0])
out=f'''# GTA VI 第一幕原生曝光与慢提交实测

2026 年 11 月 25 日，M5 Max、GPTK 4.0 beta 3、Series S 设置、Plane 0。双击{link('第一幕原生曝光启动器','启动 GTA VI 第一幕原生曝光.command')}即可复现。主画面由游戏自己的曝光、超分和合成程序生成。原生超分目标是 1920×1080，窗口截帧另有缩放。{primary} · {link('发布版运行日志',release)}

## 曝光为什么一直是零

- 原版曝光管线收到 20×12 组，每组 256 个线程。没有固定曝光时，在它之后立即读取 GPU 缓冲，16 次都是零。在另一次交替试验中，给缓冲写入能识别的起始值，原版五次都原样保留。{link('原版现场日志',b,loc(b.read_text(),'exposure immediately after dispatch'))} · {link('带初值的对照',a,loc(a.read_text(),'which="original"'))}
- 完整程序声明每组按 64 路执行。单改计数器写法后 DXIL 通过验证，但保留 64 路时，入口计数仍为零。只去掉 64 路限制也有问题，240 次预期计数会变成 480 次。把着色器的线程编号也按 32 路生成后，计数 240 次，返回的最大旧值为 239，最后一级写曝光一次。{link('各变体的原始计数',a,loc(a.read_text(),'which="uint32"'))}
- 原版把 0x0d58 的 R32_UINT 计数器归零翻成了浮点、单通道的 typed UAV 写。微软的验证器明确拒绝其写掩码。修正后对这个整数视图按要求写四个零，最终曝光仍由原程序 0x11d4 的两字缓冲写入。四次 32 路对照得到 0.867、1.113、1.246、1.392，每次第二个浮点都是预置的上次曝光 0.75。{link('独立 DXIL 验证','tmp/gpu-time-study/validator.log')} · {link('计数器翻译','victory-ag/crates/victory-shader/src/emit/lower.rs',8130)}
- 撤去起始值和空间放大后，诊断版连续 49 次读到有限曝光值，后 48 次保存的上一帧值全部对得上。超分的 1080p 半浮点目标在四次读回中，每次 6,220,800 个 RGB 分量均为有限值。发布版也直接走相同的游戏路径，本轮记录 {len(pairs)} 次曝光输出。{link('超分读回',native,loc(native_text,'native upscaler output'))} · {link('发布版曝光',release,start)}

## 哪些命令拖慢第一幕

VIctory 的 `slow host submission` 记录一批列表从提交到等完的墙钟时间。GPTK 报告的 D3D12 时间戳频率为 60，39 次列表检查中，相邻原始时间戳全部相同。原生 Metal 追踪能看到真正的计算活动。在四张耗时 365–396 ms 的慢列表里，有 359.5–388.3 ms 与该进程的 Metal 计算区间重叠。{link('平台时间戳',instr,loc(instr.read_text(),'raw list timestamp check'))} · {link('与 Metal 时间轴对齐的数据','tmp/gpu-time-study/native-compute-windows.csv')}

性能试验在同一轮依次运行各条件。以下均为整张命令列表的毫秒数，按相邻场景帧配对，不是单条着色器的独占时间。保留了参数检查及资源绑定；跳过命令仅用于找耗时，发布启动器始终执行原有命令。

| 场景阶段 | 全部执行 | 只跳过的间接计算程序 | 跳过后 |
|---|---:|---|---:|
| 前段第一组 | 232.5 | `8bbcbda6008eaa37` | 19.2 |
| 前段第二组 | 236.9 | `8bbcbda6008eaa37` | 99.0 |
| 后段 92 条间接命令 | 136.2 | `c7feccae4e01e454` | 7.9 |
| 后段 94 条间接命令 | 133.9 | `c7feccae4e01e454` | 5.8 |

前段两组的命令数量随帧略有变化，后段两组各自的命令数量一致。单独跳过 `982f3cccd5c04294` 在这四组没有类似下降。耗时会随镜头变化，这组数据还不能给着色器命名为体积云或全局光照。{link('全部逐帧对照','logs/gptk4-native-exposure-indirect-ab.csv')} · {link('对照日志',native,85549)}

## 可用状态

{stats} 人物轮廓和远景已经可见，部分人物和近景仍过暗。发布版不包含实验用的跳过命令、填曝光初值或强制清计数器逻辑。新开关 `VICTORY_SPD_WAVE32=1` 只匹配这一个曝光程序。关闭开关后，此程序生成的 DXIL 与改动前逐字节相同。着色器库本机测试 146 项通过、2 项忽略。{link('目标着色器选择','victory-ag/crates/victory-shader/src/emit/mod.rs',509)} · {link('测试结果','tmp/gpu-time-study/shader-tests.log')}
'''
(logs/'第一幕原生曝光与慢提交实测.md').write_text(out)
print('release captures',frames,'exposure count',len(pairs),'stats',stats)
