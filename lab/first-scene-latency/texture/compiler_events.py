"""Attribute retained system Metal events using client and compiler service PIDs."""
from collections import Counter
from datetime import datetime
from pathlib import Path
import json
import re
from zoneinfo import ZoneInfo

here = Path(__file__).resolve().parent
project = here.parents[1]
eastern = ZoneInfo('America/New_York')
resources = []
for source in sorted((Path.home()/'Library/Logs/DiagnosticReports').glob('MTLCompilerService*.ips')):
    with source.open() as f:
        header = json.loads(f.readline())
        ips = json.load(f)
    if not ips['captureTime'].startswith('2026-11-25 08:11:'): continue
    frame = ips['threads'][ips['faultingThread']]['frames'][0]
    resources.append(dict(file=str(source), incident=ips['incident'], capture=ips['captureTime'],
                          responsible=ips['responsibleProc'], owner=ips['responsiblePid'], service=ips['pid'],
                          exception=ips['exception']['type'], signal=ips['exception']['signal'],
                          crashing_binary=ips['usedImages'][frame['imageIndex']]['name'], offset=frame.get('imageOffset')))

def system_time(row):
    return datetime.fromisoformat(row[:23]).replace(tzinfo=eastern)

siglines=(here/'compiler-signals-key.log').read_text().splitlines()
d3d=(here/'compiler-driver-key.log').read_text().splitlines()
xpc=(here/'compiler-xpc.log').read_text().splitlines()
summary=[]
for tag,pid,provenance in [('080612-kz1g1gwi',38339,'Wine client checked in at game startup 08:06:12.832. Same client throughout this exclusive launch. Launcher wrapper PID 38336.'),
       ('080944-szt6mm4z',39527,'MTLCompilerService .ips responsibleProc explicitly victory.exe, responsiblePid=39527. Launcher wrapper PID 39525.'),
       ('081514-xdg0lmbw',41058,'Window owner PID=41058 is recorded in first-scene-latency/runs/schedule-ornp07qk/windows.json. Launcher wrapper PID 41026.')]:
    gl=next((project/'logs').glob('victory-run-*'+tag+'.log'))
    rows=gl.read_text().splitlines()
    w=next(x for x in rows if "world's first frame" in x)
    world=datetime.fromisoformat(w[:27]); wframe=int(re.search(r'frame=(\d+)',w)[1]);
    exits=[(system_time(s),int(re.search(r'\[(\d+)\]:\] exited',s)[1])) for s in siglines if f'pid/{pid}/com.apple.MTLCompilerService.' in s and 'exited due to SIGSEGV' in s]
    assert len(exits)==len({p for t,p in exits})
    matched=[s for s in d3d if f'wineloader[{pid}:' in s]
    blocks={}
    for label,pattern in [('wave32','Must be 32'),('compute_error6','Failed to compile stage Compute - error:6'),('no_op','as no-op')]:
        lines=[s for s in matched if pattern in s]
        after=[s for s in lines if system_time(s)>=world]; late=[s for s in lines if (system_time(s)-world).total_seconds()>30];
        blocks[label]=dict(total=len(lines),after_world=len(after),over_30s_after_world=len(late),ids=sorted({int(m[1]) for s in lines if (m:=re.search(r'PSO\((\d+)\)',s))}))
    times=[(t-world).total_seconds() for t,p in exits]
    def bucket(t):return 'pre_world' if t<0 else ('0_15s' if t<15 else '15_30s' if t<30 else '30_60s' if t<60 else '60_90s' if t<90 else 'over_90s')
    stage=[s for s in rows if 'material lighting uses the current scene depth' in s or 'material lighting recorded in the title scene' in s or 'spatial upscale uses the current lit material image' in s]
    daemon=next((project/'logs').glob('*'+tag+'-daemon.log')).read_text().splitlines()[-1]
    summary.append(dict(run=tag,pid=pid,ownership=provenance,world_frame=wframe,world_time=world.astimezone(eastern).isoformat(),first_exit_s=round(min(times),3),last_exit_s=round(max(times),3),
        seh_service_exits=len(exits),exits_by_world_time=dict(Counter(map(bucket,times))),service_pids_and_world_seconds=[(p,round((t-world).total_seconds(),3)) for t,p in exits],
        d3d=blocks,reported_compiler_xpc_interruptions=sum(f'wineloader[{pid}:' in s for s in xpc),
        title_FSR4_ACO_fallback_count=sum('falling back to NewBE' in s for s in rows),material_pipeline_failures=sum('material lighting pipeline failed' in s for s in rows),
        material_pipeline_records=stage,daemon_last_line=daemon))
(here/'compiler-events.json').write_text(json.dumps(dict(query_start='2026-11-25 08:06:12 -0500',query_end='2026-11-25 08:18:36 -0500',runs=summary,retained_ips=resources),indent=2)+'\n')
for e in summary: print(e['run'],e['seh_service_exits'],e['exits_by_world_time'],e['first_exit_s'],e['last_exit_s'],{k:(v['total'],v['after_world'],v['over_30s_after_world'],len(v['ids'])) for k,v in e['d3d'].items()},e['reported_compiler_xpc_interruptions'])
