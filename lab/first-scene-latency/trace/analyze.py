"""Read material-correct first-scene logs and validate frame attribution with saved pictures."""
import argparse
from collections import defaultdict
from datetime import datetime
import json
import math
from pathlib import Path
import re
from statistics import mean, median

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
STUDY = ROOT / 'tmp/material-input-zPOxz5eq'
NUM = r'([\d.]+)'
HOST = re.compile(r'host: [\d.]+ fps, ' + NUM + r' ms/frame \(' + NUM + ' preparing: copies ' + NUM +
                  ' ms ' + NUM + ' MB, textures ' + NUM + ' ms ' + NUM + ' MB; ' + NUM +
                  ' on the GPU; ' + NUM + ' copying back ' + NUM + r' MB\), ' + NUM +
                  ' draws/frame, dropped ' + NUM + r' without a host pipeline \(' + NUM +
                  ' being made\\) and ' + NUM + ' indirect')
HOST_KEYS = 'frame_ms prepare_ms copy_ms copy_mb tex_ms tex_mb submit_fence_ms back_ms back_mb draws drops compiling indirect_drops'.split()
COPY_FIELDS = [
    (['submissions'], r'copies per frame: ' + NUM + ' submissions'),
    (['refreshes','refreshed_mb'], NUM+' refreshes '+NUM+' MB'),
    (['checked_move_worker_ms','unmapped_mb'],r'moves '+NUM+' ms, '+NUM+' MB unmapped'),
    (['fallbacks'],NUM+' fallbacks'),
    (['texture_hashed_mb','texture_changed_mb'],NUM+' MB hashed, '+NUM+' MB of them changed'),
    (['whole_mb','whole_worker_ms'],r'copied whole \(not tracked\) '+NUM+' MB in '+NUM+' ms'),
    (['snapshot_mb','snapshot_worker_ms'],'snapshots '+NUM+' MB in '+NUM+' ms'),
    (['refresh_batch_ms'],'preparing: '+NUM+' ms in refresh batches'),
    (['step_copy_ms'],'refresh [\\d.]+ ms, copy '+NUM+' ms'),
    (['heap_upload_ms','prelude_ms'],'heap texture uploads '+NUM+' ms, state preludes '+NUM+' ms'),
    (['back_compared_mb','back_compare_ms'],r'copying back compared '+NUM+r' MB\s+in '+NUM+' ms'),
]
SOURCE = re.compile(r'copied from (0x[\da-f]+)\+(0x[\da-f]+) \(memory (0x[\da-f]+)\): '+NUM+'x/frame, '+NUM+' MB/frame, '+NUM+' ms/frame')
FRAME = re.compile(r'frame saved path=.*?[\\/]frame-(\d+)\.png')
STAMP = re.compile(r'^(\d{4}-\d\d-\d\dT[\d:.]+Z)')

def stats(rows,keys):
    result = {}
    for k in keys:
        values = [r[k] for r in rows if k in r]
        if values: result[k] = round(median(values),3)
    return result

def parse(name,path):
    capture, presents, host, counters = {}, [], [], []
    previous = None
    world = None
    for line in path.read_text(errors='replace').splitlines():
        ts=STAMP.match(line)
        if not ts: continue
        t=datetime.fromisoformat(ts[1]).timestamp()
        if "world's first frame" in line:
            world=int(re.search(r' frame=(\d+)',line)[1])
            world_ts=t
        if m:=FRAME.search(line):
            capture[int(m[1])] = t
        if 'title memory this frame:' in line:
            presents.append(t)
        if m:=HOST.search(line):
            row=dict(zip(HOST_KEYS,map(float,m.groups())))
            row.update(t=t,prepare_share_pct=100*row['prepare_ms']/row['frame_ms'])
            host.append(row)
        if 'copies per frame:' in line and 'refresh batches' in line:
            row={'t':t,'start':previous,'sources':[]}
            previous=t
            for keys,pattern in COPY_FIELDS:
                if not (m:=re.search(pattern,line)): raise ValueError((name,pattern,line))
                row.update(zip(keys,map(float,m.groups())))
            row['texture_changed_pct']=100*row['texture_changed_mb']/row['texture_hashed_mb'] if row['texture_hashed_mb'] else 0.0
            counters.append(row)
        if (m:=SOURCE.search(line)) and counters:
            addr,size,kind,n,mb,ms=m.groups()
            counters[-1]['sources'].append(dict(address=addr,size=int(size,16),kind=kind,calls=float(n),mb=float(mb),ms=float(ms)))
    if not world: raise ValueError(f'{name}: world marker missing')
    saved = {k-world:v for k,v in capture.items() if k>=world}
    offsets=sorted(saved)
    if len(offsets)<2: raise ValueError(f'{name}: need two world screenshot anchors')
    mapped={}
    gaps=[]
    for a,b in zip(offsets,offsets[1:]):
        found=[t for t in presents if saved[a]<=t<saved[b]]
        if len(found)!=b-a: # Empty memory counters may suppress per-present traces.
            gaps.append([a,b,len(found)])
            continue
        mapped.update({a+i:t for i,t in enumerate(found)})
    last = [t for t in presents if t>=saved[offsets[-1]]]
    if last and last[0]-saved[offsets[-1]]<.25:
        mapped[offsets[-1]]=last[0]
    def section(start,end):
        start_time=saved[start] if start is not None else world_ts
        finish=saved[end]
        hs=[r for r in host if start_time<r['t']<=finish]
        # Copy counters cover all frames since the preceding five-second report.
        cs=[r for r in counters if r['start'] is not None and start_time<=r['start'] and r['t']<=finish]
        delta=[dict(offset=i,ms=round((mapped[i]-mapped[i-1])*1000,3))
               for i in sorted(mapped) if i-1 in mapped and (start is None or i>start) and i<=end]
        cand=defaultdict(list)
        for c in cs:
            for s in c['sources']: cand[(s['address'],s['size'])].append(s)
        ranked=[]
        for (address,size),vals in cand.items():
            ranked.append(dict(address=address, size=size, samples=len(vals),**stats(vals,['calls','mb','ms'])))
        ranked.sort(key=lambda e:e['mb']*e['samples'],reverse=True)
        endpoint=start if start is not None else offsets[0]
        verified=delta and start is not None and len(delta)==end-start
        frame_intervals=None
        if verified:
            times=sorted(i['ms'] for i in delta)
            frame_intervals=dict(n=len(times),mean_ms=round(mean(times),3),median_ms=round(median(times),3),
                                 p95_ms=times[math.ceil(.95*len(times))-1])
        return dict(start=start,stop=end,screen_mean_ms=round(1000*(saved[end]-saved[endpoint])/(end-endpoint),3),
                    capture_start_for_mean=endpoint,intervals=delta,frame_intervals=frame_intervals,
                    host_report_count=len(hs),host_medians=stats(hs,HOST_KEYS+['prepare_share_pct']),
                    full_copy_windows=len(cs),copy_medians=stats(cs,[k for ks,_ in COPY_FIELDS for k in ks]+['texture_changed_pct']),
                    top_reported_sources=ranked[:8],host_rows=hs,copy_rows=cs)
    images=ROOT/'victory-frames'/path.stem.removeprefix('victory-run-')
    image_refs={k:str((images/f'frame-{world+k}.png').relative_to(ROOT)) for k in offsets}
    sections={'marker_to_last':section(None,offsets[-1]), 'capture_span':section(offsets[0],offsets[-1])}
    if 14 in offsets and 26 in offsets: sections['same_offsets_14_26']=section(14,26)
    return dict(log=str(path.relative_to(ROOT)),world=world,marker_ts=world_ts,capture_offsets=offsets,
                unmatched_anchor_counts=gaps,images=image_refs,sections=sections)

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--extra',action='append',default=[],metavar='NAME=LOG',help='Additional instrumented run')
    ap.add_argument('--output',type=Path,default=HERE/'results.json')
    args=ap.parse_args()
    out={}
    for case in ['baseline','tables','restored','cleaned']:
        run=(STUDY/(case+'.log')).read_text()
        log=Path(re.search(r'logging to (.+\.log)',run)[1])
        out[case]=parse(case,log)
        out[case]['binary_sha256']=re.search(r'SHA-256 (\S+)',run)[1]
    release = STUDY/'release-play.log'
    if release.exists():
        log=Path(re.search(r'logging to (.+\.log)',release.read_text())[1])
        out['release']=parse('release',log)
        out['release']['binary_sha256']=(STUDY/'final-binary.sha256').read_text().strip()
    for extra in args.extra:
        name,f=extra.split('=',1)
        out[name]=parse(name,Path(f).resolve())
    args.output.write_text(json.dumps(out,indent=2)+'\n')
    for name,r in out.items():
        print(name,'world',r['world'],'verified capture offsets',r['capture_offsets'],'gaps',r['unmatched_anchor_counts'])
        for key,s in r['sections'].items():
            print(key, s['host_report_count'], s['full_copy_windows'], s['frame_intervals'],s['host_medians'],
                  s['copy_medians'],s['top_reported_sources'][:1])
if __name__=='__main__': main()
