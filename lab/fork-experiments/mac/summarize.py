"""Summarize VIctory's frame counter, timestamped captures and sampled copy meters."""
from datetime import datetime, timezone
from pathlib import Path
import hashlib
import json
import math
import re
import statistics
import struct
import zlib

here = Path(__file__).resolve().parent
meter = re.compile(r'([\d.]+) fps, ([\d.]+) ms/frame \([\d.]+ preparing: copies ([\d.]+) ms ([\d.]+) MB,.*?, (\d+) draws/frame, dropped (\d+)')

def frame_info(path):
    data = path.read_bytes()
    width, height, depth, color = struct.unpack('>IIBB', data[16:26])
    assert depth == 8 and color == 2, 'expected upstream RGB8 PNG'
    i = 8
    compressed = bytearray()
    while i < len(data):
        size = struct.unpack('>I', data[i:i+4])[0]
        name = data[i+4:i+8]
        if name == b'IDAT':
            compressed.extend(data[i+8:i+8+size])
        i += 12 + size
    raw = zlib.decompress(compressed)
    stride = 1 + width*3
    assert len(raw) == stride*height and all(raw[y*stride]==0 for y in range(height))
    pixels = b''.join(raw[y*stride+1:(y+1)*stride] for y in range(height))
    sample_rgb = set()
    for offset in range(0, len(pixels), 3*53):
        sample_rgb.add(pixels[offset:offset+3])
    return dict(name=path.name,size=[width,height],bytes=len(data),
                rgb_mean_0_255=round(statistics.fmean(pixels),3),sample_distinct_rgb=len(sample_rgb),
                rgb_sha256=hashlib.sha256(pixels).hexdigest())

def one(folder):
    meta = json.loads((folder/'run.json').read_text())
    world = None
    presented = {}
    saved = {}
    meters = []
    lines = (folder/'game.log').read_text(errors='replace').splitlines()
    last_timer = last_frame = None
    for line in lines:
        if not re.match(r'^\d{4}-',line):
            continue
        stamp = datetime.fromisoformat(line.split()[0]).timestamp()
        if 'frame presented (' in line:
            match = re.search(r' frame=(\d+)',line)
            if match:
                last_frame = int(match[1])
                presented[last_frame]=stamp
        if 'world\'s first frame' in line:
            world = int(re.search(r' frame=(\d+)',line)[1])
        if found := re.search(r'frame saved path=.*[\\/]frame-(\d+)\.png',line):
            saved[int(found[1])] = stamp
        if m := meter.search(line):
            meters.append(dict(utc=line.split()[0],fps=float(m[1]),frame_ms=float(m[2]),copy_ms=float(m[3]),
                               copy_mb=float(m[4]),draws=int(m[5]),dropped_draws=int(m[6]),reported_frame=last_frame))
            last_timer = stamp
    result = dict(dir=folder.name,run=meta,world_start=world,largest_reported_frame=max(presented,default=None),
                  last_reported_time=lines[-1][:27] if lines else None,meters=meters,world_intervals_ms={},world_images={})
    for start,end in [(64,128),(128,256),(256,512),(512,1024),(1024,2048)]:
        if start in presented and end in presented and (world is None or end<world):
            result.setdefault('loading',{})[f'{start}-{end}_wall_ms_per_frame']=round(1000*(presented[end]-presented[start])/(end-start),3)
    if world:
        for off in range(2,13):
            f=world+off
            if f in saved:
                if off>2 and f-1 in saved:
                    result['world_intervals_ms'][str(off)] = round((saved[f]-saved[f-1])*1000,3)
                if off in (2,6,8,12):
                    result['world_images'][str(off)] = frame_info(folder/'frames'/f'frame-{f}.png')
        values=list(result['world_intervals_ms'].values())
        if values:
            result['world_stats'] = dict(n=len(values),mean_ms=round(statistics.fmean(values),2),
                                         p95_nearest_rank_ms=sorted(values)[math.ceil(.95*len(values))-1],max_ms=max(values))
        if world+2 in saved:
            sampled = [row for row in meters if saved[world+2] <= datetime.fromisoformat(row['utc']).timestamp() <= saved.get(world+12,float('inf'))]
            if sampled:
                result['world_meter_medians']={key:statistics.median(row[key] for row in sampled)
                                               for key in ('copy_ms','copy_mb','draws','dropped_draws','frame_ms')}
                result['world_meter_medians']['samples']=len(sampled)
    if last_timer:
        result['elapsed_to_last_meter_s']=round(last_timer-datetime.fromisoformat(meta['started_utc']).timestamp(),2)
    if saved:
        result['last_capture']=max(saved)
        result['capture_times_utc']={k:datetime.fromtimestamp(v,timezone.utc).isoformat() for k,v in saved.items()}
        if 'elapsed_s' in meta:
            end=datetime.fromisoformat(meta['started_utc']).timestamp()+meta['elapsed_s']
            result['seconds_without_further_capture_at_end']=round(end-max(saved.values()),3)
    return result

records = [one(file.parent) for file in sorted(here.glob('*/run.json'))]
(here/'results.json').write_text(json.dumps(records,indent=2)+'\n')
for r in records:
    print(r['dir'],r['world_start'],r.get('world_stats'),r.get('world_meter_medians'),r.get('loading'),r.get('last_capture'),r['run'].get('stop'),flush=True)

def compare_window(record, first, last):
    world=record['world_start']
    times=record['capture_times_utc']
    begin=datetime.fromisoformat(times[world+first])
    end=datetime.fromisoformat(times[world+last])
    intervals=[record['world_intervals_ms'][str(k)] for k in range(first+1,last+1)]
    meters=[row for row in record['meters'] if begin <= datetime.fromisoformat(row['utc']) <= end]
    return dict(run=record['dir'],commit=record['run']['commit'],binary_sha256=record['run']['binary_sha256'],
                world_start_frame=world,same_relative_window=[first,last],n_intervals=len(intervals),
                intervals_ms=intervals,mean_ms=round(statistics.mean(intervals),2),
                p95_nearest_rank_ms=sorted(intervals)[math.ceil(.95*len(intervals))-1],
                meters=meters,meter_medians={k:statistics.median(x[k] for x in meters)
                for k in ('frame_ms','copy_ms','copy_mb','draws','dropped_draws')} if meters else None,
                world_images=record['world_images'],last_capture=record['last_capture'],
                seconds_without_further_capture_at_end=record['seconds_without_further_capture_at_end'])

# Freeze the original A/B pair as a separate result even when the launcher is used for other builds.
names=('default-0o74wip1','tracking-0q0ax4ai')
pair=[r for r in records if r['dir'] in names and 'elapsed_s' in r['run']]
if len(pair)==2:
    (here/'mac-pair.json').write_text(json.dumps([compare_window(r,2,8) for r in pair],indent=2)+'\n')
