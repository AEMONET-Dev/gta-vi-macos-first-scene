"""Align the release's per-present memory markers with launchd's Metal exits."""
from datetime import datetime
from pathlib import Path
import json
import re
import statistics
from zoneinfo import ZoneInfo

here = Path(__file__).resolve().parent
root = here.parents[1]
log = next((root/'logs').glob('victory-run*080612-kz1g1gwi.log'))
zone = ZoneInfo('America/New_York')
anchors = {}
presents = []
for line in log.read_text().splitlines():
    if not line.startswith('2026-11-25T'): continue
    clock = datetime.fromisoformat(line.split()[0])
    if "world's first frame" in line: world = int(re.search(r'frame=(\d+)',line)[1])
    if 'title memory this frame:' in line: presents.append(clock)
    if match := re.search(r'frame saved path=.*?\\frame-(\d+)\.png',line):
        anchors[int(match[1])] = clock

frames = {}
world_captures=sorted(i for i in anchors if i>=world)
for left,right in zip(world_captures,world_captures[1:]):
    enclosed = [t for t in presents if anchors[left]<=t<anchors[right]]
    assert len(enclosed)==right-left,(left,right,len(enclosed))
    frames.update({left+i:t for i,t in enumerate(enclosed)})
last=world_captures[-1]
frames[last] = next(t for t in presents if t>=anchors[last]); assert (frames[last]-anchors[last]).total_seconds()<.25
signal_rows=(here/'compiler-signals-key.log').read_text().splitlines()
exits = [(datetime.fromisoformat(s[:23]).replace(tzinfo=zone),int(re.search(r'\[(\d+)\]:\] exited',s)[1]))
         for s in signal_rows if 'pid/38339/com.apple.MTLCompilerService.' in s and 'exited due to SIGSEGV' in s]
rows = []
for offset in range(18,41):
    n=world+offset; start,end=frames[n-1],frames[n]
    service_ids=[pid for clock,pid in exits if start<clock<=end]
    rows.append(dict(present_offset=offset,frame=n,begin=start.isoformat(),end=end.isoformat(),
                     frame_ms=round((end-start).total_seconds()*1000,3),exit_service_pids=service_ids))
# Compare independent existing performance trace frame durations against the aligned timestamps.
existing=json.loads((here.parent/'trace/results.json').read_text())
expected=existing['release']['sections']['capture_span']['intervals']
assert [(x['offset'],x['ms']) for x in expected]==[(x['present_offset'],x['frame_ms']) for x in rows]
summary={}
for label,signalled in [('with_sigsegv',True),('without_sigsegv',False)]:
    group=[row for row in rows if bool(row['exit_service_pids'])==signalled]
    ts=[x['frame_ms'] for x in group]; summary[label]=dict(n=len(group),exits=sum(len(x['exit_service_pids']) for x in group),
                median_ms=round(statistics.median(ts),3),min_ms=min(ts),max_ms=max(ts),ms_over_2000=sum(x>2000 for x in ts),
                frame_offsets=[x['present_offset'] for x in group])
result=dict(run=log.name,pid=38339,world=world,anchors=world_captures,elapsed_ms=round(sum(r['frame_ms'] for r in rows),3),summary=summary,frames=rows)
(here/'compiler-frame-overlap.json').write_text(json.dumps(result,indent=2)+'\n'); print(summary)
