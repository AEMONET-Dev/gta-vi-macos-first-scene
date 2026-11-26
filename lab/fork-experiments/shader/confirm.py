"""Confirm identical outputs before computing timings; run the same candidate on the expensive shader."""
import json
import re
import statistics
import subprocess
import run
from repeat import measure

def main():
    rows=measure('confirm-dce','dce',True,loops=50)
    stats={}
    for name,v in rows.items():
        logs=(run.HERE/('confirm-dce-'+name+'-repeat.log')).read_text()
        ms=[float(s) for s in re.findall(r'compute-ms=([\d.]+)',logs)]
        assert len(ms)==len(v)
        stats[name]={'trials':len(v),'exact_trials':sum(row['mismatching_bytes']==0 for row in v),
                     'worst_wrong_bytes':max(row['mismatching_bytes'] for row in v),
                     'median_ms':statistics.median(ms),'p95_ms':sorted(ms)[47]}
    shader, manifest=run.SAMPLES['8bb']
    msl=run.translations(shader,'dce',['--dead-code-eliminate'],False)
    env={**run.ENVS,'REPLAY_PRECISE':'1'}
    rows=[]
    for i in range(4):
        output=run.HERE/'confirm-dce-8bb'
        with (run.HERE/'confirm-dce-8bb.log').open('w' if i==0 else 'a') as f:
            subprocess.run([run.HERE/'replay',manifest,msl,output],env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
        r=run.compare(manifest,output)
        rows.append(r)
    (run.HERE/'dce-8bb-details.json').write_text(json.dumps(rows,indent=2)+'\n')
    stats['8bb']={'trials':len(rows),'exact_outputs':[row['exact'] for row in rows],'wrong_bytes':[row['mismatching_bytes'] for row in rows]}
    print(stats)
    (run.HERE/'confirmed.json').write_text(json.dumps(stats,indent=2)+'\n')
if __name__=='__main__':
    main()
