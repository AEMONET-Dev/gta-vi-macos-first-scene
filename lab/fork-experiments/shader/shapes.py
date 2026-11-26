"""Test native 1D, 2D, array and 3D descriptor shapes and include an intentional one-group control."""
import json
import subprocess
import run

def main():
    source=(run.OLD/'opensource-replay.m').read_text()
    pat='d.textureType=td3?MTLTextureType3D:MTLTextureType2DArray;'
    assert source.count(pat)==1
    source=source.replace(pat,'d.textureType=td3 ? MTLTextureType3D : ([w[1]intValue]==2 ? MTLTextureType1D : ([w[1]intValue]==4 ? MTLTextureType2D : MTLTextureType2DArray));')
    new=run.HERE/'shapes.m'
    new.write_text(source)
    exe=run.HERE/'shapes'
    run.run(['xcrun','clang','-O2','-fobjc-arc','-framework','Metal','-framework','Foundation',new,'-o',exe])
    data={}
    for sample,label,host,flags in [('8bb','shape-exact',exe,{}),('8bb','shape-safe',exe,{'REPLAY_PRECISE':'1'}),('c7a','one-group',run.HERE/'replay',{'REPLAY_PRECISE':'1','REPLAY_GROUPS_X':'1'})]:
        shader,m=run.SAMPLES[sample]
        prog=run.HERE/(shader+'-dce.metal')
        out=run.HERE/label
        with (run.HERE/(label+'.log')).open('w') as f:
            subprocess.run([host,m,prog,out],check=True,env={**run.ENVS,**flags},stdout=f,stderr=subprocess.STDOUT)
        r=run.compare(m,out)
        data[label]=r
        print(label,r['exact'],r['mismatching_bytes'],flush=True)
    (run.HERE/'shapes-and-control.json').write_text(json.dumps(data,indent=2)+'\n')
if __name__=='__main__':
    main()
