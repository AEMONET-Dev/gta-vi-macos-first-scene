"""Repeat the same selected settings for both shaders and measure GPU command time."""
import json
import re
import statistics
import subprocess
import run
def main():
    results={}
    for name,(shader,manifest) in run.SAMPLES.items():
        loops=24 if name!='8bb' else 12
        log=run.HERE/(name+'-selected.log')
        out=run.HERE/('selected-'+name)
        trials=[]
        with log.open('w') as f:
            for i in range(loops):
                run.run([run.HERE/'modes',manifest,run.HERE/(shader+'-dce.metal'),out],env={**run.ENVS,'AUDIT_MODE':'0','AUDIT_FN':'0'},stdout=f,stderr=subprocess.STDOUT)
                row=run.compare(manifest,out)
                trials.append({'exact_outputs':row['exact'],'bytes_compared':row['bytes_compared'],'mismatching_bytes':row['mismatching_bytes'],'mismatches':{k:v['mismatching_bytes'] for k,v in row['outputs'].items() if v['mismatching_bytes']}})
        times=[float(t) for t in re.findall(r'compute-ms=([\d.]+)',log.read_text())]
        assert len(times)==loops
        for t,ms in zip(trials,times):
            t['gpu_ms']=ms
        summary={'trials':loops,'fully_exact_trials':sum(row['mismatching_bytes']==0 for row in trials),'per_trial_exact_outputs':[r['exact_outputs'] for r in trials],'median_gpu_ms':statistics.median(times),'p95_gpu_ms':sorted(times)[int(__import__('math').ceil(len(times)*.95))-1],'trial_results':trials,'last_run':row}
        results[name]=summary
        print(name,loops,summary['per_trial_exact_outputs'],summary['median_gpu_ms'],flush=True)
    (run.HERE/'selected.json').write_text(json.dumps(results,indent=2)+'\n')
if __name__=='__main__':
    main()
