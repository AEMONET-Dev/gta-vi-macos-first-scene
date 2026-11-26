"""Separate arithmetic reassociation from transcendental precision using native resource dimensions."""
import json
import subprocess
import run
def main():
    src=(run.HERE/'shapes.m').read_text()
    before='NSString*source='
    assert src.count(before)==1
    src=src.replace(before, 'opts.mathMode=(MTLMathMode)atoi(getenv("AUDIT_MODE")); opts.mathFloatingPointFunctions=(MTLMathFloatingPointFunctions)atoi(getenv("AUDIT_FN"));\n'+before)
    path=run.HERE/'modes.m'; path.write_text(src)
    exe=run.HERE/'modes'
    run.run(['xcrun','clang','-O2','-fobjc-arc','-framework','Metal','-framework','Foundation',path,'-o',exe])
    rows={}
    for sample in ['8bb','c7a','c7b']:
        shader,manifest=run.SAMPLES[sample]
        for mode in (0,1,2):
            for fn in (0,1):
                label=f'{sample}-mode-{mode}-fn-{fn}'
                out=run.HERE/('output-mode-'+sample)
                with (run.HERE/(label+'.log')).open('w') as f:
                    run.run([exe,manifest,run.HERE/(shader+'-dce.metal'),out],env={**run.ENVS,'AUDIT_MODE':str(mode),'AUDIT_FN':str(fn)},stdout=f,stderr=subprocess.STDOUT)
                r=run.compare(manifest,out); rows[label]=r
                mismatches={k:v['mismatching_bytes'] for k,v in r['outputs'].items() if v['mismatching_bytes']}
                print(label,r['exact'],mismatches,flush=True)
    (run.HERE/'modes.json').write_text(json.dumps(rows,indent=2)+'\n')
if __name__=='__main__':
    main()
