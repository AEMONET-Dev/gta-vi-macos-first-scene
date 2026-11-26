"""Verify repeated native control and Metal object lifetime against the game captures."""
import json
import subprocess
import run
from repeat import measure

def apple_control():
    folder=run.BASE/'tmp/actual-dispatch-b8_w769p'
    result={}
    for sample in ['c7a','c7b']:
        output=run.HERE/('native-'+sample)
        checks=[]
        with (run.HERE/('native-'+sample+'.log')).open('w') as f:
            for _ in range(10):
                subprocess.run([folder/'native-replay',run.SAMPLES[sample][1],folder/'c7.metallib',output],env=run.ENVS,stdout=f,stderr=subprocess.STDOUT,check=True)
                r=run.compare(run.SAMPLES[sample][1],output)
                checks.append([r['exact'],r['mismatching_bytes']])
        result[sample]=checks;print('native',sample,checks,flush=True)
    (run.HERE/'native-control.json').write_text(json.dumps(result,indent=2)+'\n')
def lifetime():
    source=(run.OLD/'opensource-replay.m').read_text()
    old='NSMutableArray *keep=[NSMutableArray'
    new='__attribute__((objc_precise_lifetime)) NSMutableArray *keep=[NSMutableArray'
    assert source.count(old)==1
    source=source.replace(old,new)
    file=run.HERE/'lifetime.m'
    file.write_text(source)
    exe=run.HERE/'lifetime'
    subprocess.run(['xcrun','clang','-fobjc-arc','-O2','-framework','Metal','-framework','Foundation',file,'-o',exe],check=True)
    measure('keep-alive','safe',True,loops=12,host=exe)
if __name__=='__main__':
    apple_control()
    lifetime()
