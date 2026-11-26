from pathlib import Path
import ast,re,collections,statistics
s=Path('logs/victory-run-gptk4-time-spd-crossover.log').read_text(errors='replace').splitlines()
for x in s:
 if 'independent DXIL validation' in x or 'test pipeline refused' in x or 'exposure immediately after dispatch' in x:print(x[:900])
rows=collections.defaultdict(list)
for x in s:
 if 'measured workload list' not in x:continue
 m=re.search(r'study=(\d+) frame=(\d+) cmds=(\d+) uploads=(\d+) indirect=(\d+) us=(\d+) seen=(.*)',x);study,frame,cmds,uploads,indirect,us=map(int,m.groups()[:6]);t=ast.literal_eval(m[7]);rows[study].append((frame,cmds,uploads,indirect,round(us/1000,1)))
 if t:
  pts=[(a[0],b[0],round((b[1]-a[1])/1000,1)) for a,b in zip(t,t[1:]) if b[1]>a[1]];print('probe:',frame,cmds,indirect,round(us/1000,1),'first, last',t[0],t[-1],'gaps',sorted(pts,key=lambda x:-x[2])[:3])
for k,v in rows.items():print('mode',k,'(frame, cmds, uploads, indirect, ms)',v)
