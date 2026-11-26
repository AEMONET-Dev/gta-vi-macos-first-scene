"""Recompile both accepted-format translations, rebuild the host, and rerun the selected experiment."""
import json
import subprocess
import sys
import run
import final_replay

def main():
    for shader in {v[0] for v in run.SAMPLES.values()}:
        run.translations(shader,'dce',['--dead-code-eliminate'],False)
    run.run(['xcrun','clang','-O2','-fobjc-arc','-framework','Metal','-framework','Foundation',run.HERE/'modes.m','-o',run.HERE/'modes'])
    final_replay.main()
    env={**run.ENVS,'AUDIT_MODE':'0','AUDIT_FN':'0','REPLAY_GROUPS_X':'1'}
    shader,manifest=run.SAMPLES['c7a']
    out=run.HERE/'selected-negative'
    with (run.HERE/'selected-negative.log').open('w') as f:
        subprocess.run([run.HERE/'modes',manifest,run.HERE/(shader+'-dce.metal'),out],check=True,env=env,stdout=f,stderr=subprocess.STDOUT)
    row=run.compare(manifest,out)
    assert row['mismatching_bytes']>0
    (run.HERE/'selected-negative.json').write_text(json.dumps(row,indent=2)+'\n')
    print('one workgroup',row['exact'],row['mismatching_bytes'],flush=True)

if __name__=='__main__':
    main()
