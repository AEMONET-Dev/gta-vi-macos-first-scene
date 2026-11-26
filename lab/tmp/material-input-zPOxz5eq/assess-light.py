from pathlib import Path
import sys,re,json
from PIL import Image,ImageDraw
import numpy as np
s=Path(__file__).resolve().parent; root=s.parents[1]; out=[]
for case in sys.argv[1:]:
 log=Path(re.search('logging to (.+log)',(s/(case+'.log')).read_text())[1]); txt=log.read_text(errors='replace')
 start=re.search(r"world's first frame [^\n]+?frame=(\d+)",txt)
 if not start:continue
 world=int(start[1]); folder=root/'victory-frames'/log.stem.removeprefix('victory-run-')
 sheet=Image.new('RGB',(650,165*5)); d=ImageDraw.Draw(sheet);i=0
 for p in sorted(folder.glob('frame-*.png')):
  frame=int(p.stem.split('-')[1])
  if not world+14<=frame<=world+50:continue
  try:
   im=Image.open(p).convert('RGB');a=np.asarray(im.resize((480,270))).astype(float)
  except OSError:continue
  # Same screen location on each camera angle. Bright, blue-white pixels in the spot's interior.
  area=a[160:188,212:233];r,g,b=area.transpose(2,0,1)
  spot=int(((b>226)&(g>160)&(b>=r+5)&(b>=g+6)).sum())
  ring=a[148:198,199:245].copy()
  detail=dict(case=case,frame=frame,offset=frame-world,spot=spot,luma=round(a.mean(),1),path=str(p))
  out.append(detail)
  x=(i%2)*325;y=(i//2)*165;i+=1
  im.thumbnail((265,134));sheet.paste(im,(x,y));box=Image.fromarray(area.astype('uint8')).resize((50,64))
  sheet.paste(box,(x+265,y+40));d.text((x,y+134),f'{frame}  offset {frame-world}  spot {spot}')
 if i: sheet.crop((0,0,650,165*((i+1)//2))).save(s/(case+'-spots.jpg'))
(s/'spot-statistics.json').write_text(json.dumps(out,indent=2))
for a in out:print(a['case'],a['offset'],a['spot'],a['luma'])
