"""Collect launch, frame and shader outcomes for each isolated full-title test."""
from datetime import datetime
from pathlib import Path
import hashlib,json,re,statistics,subprocess
here=Path(__file__).resolve().parent
runs=json.loads((here/"extra-games.json").read_text())
rows=[]
for run in runs:
    folder=here/run["path"]
    meta=json.loads((folder/"run.json").read_text())
    log=(folder/"game.log").read_text(errors="replace")
    text=re.sub(r'\x1b\[[;\d]*m','',log)
    def hits(pattern): return [int(x) for x in re.findall(pattern,text)]
    frame=hits(r"frame presented [^\n]*? frame=(\d+)")
    saved=hits(r"frame saved path=.*?[\\/]frame-(\d+)\.png")
    world=hits(r"(?:world's first frame|the first frame of 500 draws or more)[^\n]*?frame=(\d+)")
    last = next((s.split()[0] for s in reversed(text.splitlines()) if re.match(r"^\d{4}-\d\d-\d\dT",s)),None)
    ready = 'host D3D12 device ready' in text
    if (folder/'backend.json').exists(): backend=json.loads((folder/'backend.json').read_text())['graphics_files']
    else: backend=[]
    waves=[s for s in text.splitlines() if 'needs waves of 64' in s]
    row=dict(mode=run['mode'],path=run['path'],commit=meta.get('commit'),binary_sha256=meta.get('binary_sha256'),
             backend=meta.get('backend'),host_device_logged=ready,loaded_files=backend,elapsed_s=meta.get('elapsed_s'),
             stop=meta.get('stop'),presented_milestones=frame,captured_frames=saved,world_markers=world,
             last_log_utc=last,rosetta_error='rosetta error:' in text,wave64_rejections=len(waves),
             archive_write_failures=text.count('Failed to write the pipeline archive'),
             mullion_pipeline_archives=len(list((folder/'mullion-cache').glob('*.metallib'))))
    stamps = {}
    for line in text.splitlines():
        match = re.search(r"frame saved path=.*?[\\/]frame-(\d+)\.png",line)
        if match and re.match(r"^\d{4}-",line):
            stamps[int(match[1])] = datetime.fromisoformat(line.split()[0]).timestamp()
    pictures = list((folder/'frames').glob('frame-*.png'))
    hashes = {hashlib.sha256(image.read_bytes()).digest() for image in pictures}
    row['screenshot_count'] = len(pictures)
    row['unique_screenshot_files'] = len(hashes)
    if len(hashes) == 1:
        raw = subprocess.check_output(['ffmpeg','-v','error','-i',str(pictures[0]),'-f','rawvideo','-pix_fmt','rgb24','-'])
        row['all_screenshots_black'] = not any(raw)
    if world:
        world_frame = world[0]
        row['world_screenshot_intervals_ms'] = {
            str(offset):round(1000*(stamps[world_frame+offset]-stamps[world_frame+offset-1]),3)
            for offset in range(3,13) if world_frame+offset in stamps and world_frame+offset-1 in stamps}
        intervals = list(row['world_screenshot_intervals_ms'].values())
        if intervals:
            row['world_mean_ms'] = round(statistics.mean(intervals),3)
        row['world_pixels'] = {}
        for offset in (2,6,8,12):
            f = folder/'frames'/f"frame-{world_frame+offset}.png"
            if f.is_file():
                raw = subprocess.check_output(['ffmpeg','-v','error','-i',str(f),'-f','rawvideo','-pix_fmt','rgb24','-'])
                row['world_pixels'][str(offset)] = dict(rgb_bytes=len(raw),rgb_max=max(raw),sha256=hashlib.sha256(raw).hexdigest())
    rows.append(row)
(here/'game-results.json').write_text(json.dumps(rows,indent=2)+'\n')
for row in rows:
    print(row['mode'],row['path'],row['elapsed_s'],row['world_markers'],max(row['captured_frames'],default=None),
          row['rosetta_error'],row['wave64_rejections'],row['archive_write_failures'])
