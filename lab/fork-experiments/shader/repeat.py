"""Use fresh GPU resources for every replay, including intentional configuration controls."""
import json
import os
import subprocess
import sys
from pathlib import Path
import run
from time import perf_counter

def measure(label, shader_variant, half_mode, views=False, loops=5, host=None, envadd=None):
    output_data = {}
    env = {**run.ENVS, **({'REPLAY_PRECISE':'1'} if half_mode else {}),
           **({'REPLAY_TYPED_FLOAT':'1','REPLAY_TYPED_VIEWS':'1'} if views else {}), **(envadd or {})}
    for name in ['c7a','c7b']:
        shader,manifest=run.SAMPLES[name]
        log=run.HERE/(label+'-'+name+'-repeat.log')
        rows=[]
        for i in range(loops):
            output=run.HERE/('repeat-'+label+'-'+name)
            generated=run.HERE/(shader+'-'+shader_variant+'.metal')
            with log.open('w' if i==0 else 'a') as f:
                subprocess.run([host or run.HERE/'replay',manifest,generated,output],check=True,env=env,stdout=f,stderr=subprocess.STDOUT)
            data=run.compare(manifest,output)
            rows.append({'exact':data['exact'],'mismatching_bytes':data['mismatching_bytes'],'outputs':{k:v['mismatching_bytes'] for k,v in data['outputs'].items()}})
        output_data[name]=rows
        print(label,name,[(v['exact'],v['mismatching_bytes']) for v in rows],flush=True)
    (run.HERE/(label+'-repeat.json')).write_text(json.dumps(output_data,indent=2)+'\n')
    return output_data
if __name__=='__main__':
    measure('fork-standard','fork',False)
    measure('safe-standard','safe',True)
    measure('safe-views','safe',True,True)
