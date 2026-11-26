from pathlib import Path
import json,statistics,re
here=Path(__file__).resolve().parent
project=here.parents[1]
study=project/'tmp/material-input-zPOxz5eq'
host = re.compile(r' ([\d.]+) fps, ([\d.]+) ms/frame \(([\d.]+) preparing: copies ([\d.]+) ms ([\d.]+) MB, textures ([\d.]+) ms ([\d.]+) MB; ([\d.]+) on the GPU; ([\d.]+) copying back [\d.]+ MB\), (\d+) draws/frame')
colors = {}
for case,version in [('restored','075504'),('cleaned','080223'),('release','080612'),('texture_candidate','080944')]:
    path=max((project/'logs').glob('*'+version+'*.log'),key=lambda p:p.stat().st_size)
    all_lines=path.read_text(errors='replace').splitlines()
    start=next(i for i,s in enumerate(all_lines) if "world's first frame" in s)
    world=int(re.search(r'frame=(\d+)',all_lines[start])[1])
    lines=all_lines[start:]
    meters=[]; hashed=[]; copies=[]; uploads=[]; partial=[]
    for line in lines:
        if m:=host.search(line):
            meters.append([float(x) for x in m.groups()])
        if m:=re.search(r'textures ([\d.]+) MB watched and changed, ([\d.]+) MB hashed, ([\d.]+) MB of them changed',line):
            hashed.append(list(map(float,m.groups())))
        if m:=re.search(r'heap texture uploads ([\d.]+) ms',line):
            copies.append(float(m[1]))
        if m:=re.search(r'heap views: (\d+) draws/frame index the heap \((\d+) writing through it\), (\d+) textures uploaded/frame',line):
            if int(m[1]): uploads.append(int(m[3]))
        if 'uploaded resident levels after whole-texture read failed' in line:
            m=re.search(r'first=(\d+) bytes=(\d+) size=(\d+) n=(\d+)',line)
            partial.append({'first':int(m[1]),'resident':int(m[2]),'requested':int(m[3]),'nth':int(m[4])})
    keys=['fps','frame_ms','prepare_ms','buffer_copy_ms','buffer_copy_MB','texture_ms','texture_MB','queue_wait_ms','writeback_ms','draws']
    colors[case]={'file':path.name,'world_frame':world,'samples':len(meters),
                  'host_meter_medians':dict(zip(keys,[round(statistics.median(x),3) for x in zip(*meters)])),
                  'texture_stat_samples':len(hashed),
                  'texture_stat_medians':dict(zip(['watched_changed_MB','checked_MB','changed_MB'],[round(statistics.median(x),3) for x in zip(*hashed)])),
                  'one_heap_upload_wall_ms_median_per_meter':round(statistics.median(copies),3),
                  'highest_logged_recovery_index':max((x['nth'] for x in partial),default=None),
                  'sampled_recoveries':partial}
(here/'texture-metrics.json').write_text(json.dumps(colors,indent=2)+'\n')
for k,a in colors.items(): print(k,a['world_frame'],a['samples'],a['host_meter_medians'],a['texture_stat_medians'],a['highest_logged_recovery_index'])
