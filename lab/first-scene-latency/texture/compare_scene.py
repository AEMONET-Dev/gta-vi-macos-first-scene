"""Compare first-scene captures and nearby runtime windows of two completed runs."""
from datetime import datetime
from pathlib import Path
import json
import re
from statistics import median

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parents[1]
ids = ('080612-kz1g1gwi', '080944-szt6mm4z')
worlds = (1609, 1726)
folders = [next((PROJECT/'victory-frames').glob('*'+tag)) for tag in ids]
logs = [next((PROJECT/'logs').glob('victory-run*'+tag+'.log')) for tag in ids]
pairs = [
    ('人物：举手的近景', 'frame', (1632,1750), '相邻动作，脸、手和衣料可辨'),
    ('同一近景的材质输入', 'base', (1632,1750), '看手臂、上衣与身后街道'),
    ('同一近景的照明输入', 'light', (1632,1750), '游戏送入合成的光照图'),
    ('街景：不同机位', 'frame', (1649,1762), '仅核对建筑、路面与人物能否呈现'),
    ('两次都出现的蓝白亮斑', 'frame', (1639,1774), '发生时刻不同，不能逐像素比对'),
]
font_path = '/System/Library/Fonts/STHeiti Medium.ttc'
font = ImageFont.truetype(font_path, 17)
small_font = ImageFont.truetype(font_path, 13)
entries = []
sheet = Image.new('RGB', (790, 38+len(pairs)*233), '#f8f8f5')
draw = ImageDraw.Draw(sheet)
draw.text((15,9),'正式光影版     08:06:12',font=font,fill='#15212c')
draw.text((403,9),'仅改纹理读取     08:09:44',font=font,fill='#15212c')

def image_path(folder, number, stage):
    return folder/f'frame-{number}.png' if stage=='frame' else next(folder.glob(f'material-{number}-*-{stage}.png'))

def luma(img):
    array=np.asarray(img.resize((112,64),Image.Resampling.LANCZOS),float)
    return (array@np.array([.2126,.7152,.0722]))[4:-4,4:-4]

for idx,(title,stage,numbers,caveat) in enumerate(pairs):
    main=[]
    paths=[]
    bright_spots=[]
    y=38+233*idx
    draw.text((15,y),title+'   '+caveat,font=font,fill='#15212c')
    for col,(folder,n,base) in enumerate(zip(folders,numbers,worlds)):
        path=image_path(folder,n,stage)
        with Image.open(path) as src:
            img=src.convert('RGB')
        main.append(luma(img)); paths.append(str(path))
        s=np.asarray(img.resize((480,270)),float)
        r,g,b=s[160:188,212:233].transpose(2,0,1)
        bright_spots.append(int(((b>226)&(g>160)&(b>=r+5)&(b>=g+6)).sum()))
        img.thumbnail((374,182),Image.Resampling.LANCZOS)
        x=15+col*388
        sheet.paste(img,(x,y+25))
        draw.text((x,y+208),f'帧 {n}    入场后第 {n-base} 帧',font=small_font,fill='#364350')
    a,b=main
    entries.append(dict(stage=stage,description=title,paths=paths,frames=numbers,world_offsets=[n-w for n,w in zip(numbers,worlds)],
                        luminance_correlation=round(float(np.corrcoef(a.flatten(),b.flatten())[0,1]),3),
                        mean_luma_0_to_255=[round(float(v.mean()),1) for v in main],
                        spot_pixels=bright_spots))
sheet.save(HERE/'scene-pairs.jpg',quality=92,subsampling=0)

columns = ['frame_ms','prepare_ms','buffer_ms','buffer_MB','texture_ms','texture_MB','queue_wait_ms','writeback_ms','draws']
meter = re.compile(r' fps, ([\d.]+) ms/frame \(([\d.]+) preparing: copies ([\d.]+) ms ([\d.]+) MB, textures ([\d.]+) ms ([\d.]+) MB; ([\d.]+) on the GPU; ([\d.]+) copying back [\d.]+ MB\), (\d+) draws/frame')
def timecode(line):
    return datetime.fromisoformat(line.split()[0].replace('Z','+00:00'))
runs=[]
for file,world in zip(logs,worlds):
    rows=file.read_text().splitlines(); start=next(i for i,s in enumerate(rows) if "world's first frame" in s); rows=rows[start:]
    t0=timecode(rows[0]); frames={}; meters=[]
    for s in rows:
        cap=re.search(r'capture: frame saved .*\\frame-(\d+)\.png',s)
        if cap: frames[int(cap[1])-world]=(timecode(s)-t0).total_seconds()
        m=meter.search(s)
        if m:meters.append(((timecode(s)-t0).total_seconds(),list(map(float,m.groups()))))
    until=frames[20]
    early=[row for t,row in meters if 0<t<=until]
    elapsed={str(k):round(v,3) for k,v in sorted(frames.items())}
    near=next(t for t in frames if world+t in (1632,1750)); tm=frames[near]
    adjacent=sorted(meters,key=lambda row:abs(row[0]-tm))[:2]
    runs.append(dict(log=str(file),world_frame=world,world_utc=t0.isoformat(),saved_after_first_world_s=elapsed,
                     meter_windows_through_world_plus_20=len(early),through_world_plus_20_median=dict(zip(columns,[round(median(col),2) for col in zip(*early)])),
                     matched_character_meters=[dict(distance_from_screenshot_seconds=round(t-tm,2),**dict(zip(columns,row))) for t,row in adjacent]))
(HERE/'scene-comparison.json').write_text(json.dumps(dict(runs=runs,pairs=entries),ensure_ascii=False,indent=2)+'\n')
for run in runs: print(run)
for pair in entries: print(pair['description'],pair['luminance_correlation'],pair['mean_luma_0_to_255'],pair['spot_pixels'])
