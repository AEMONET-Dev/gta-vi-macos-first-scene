"""Record per-thread dispatch, destination and completion when a repeat misses a tile."""
import json
import os
from pathlib import Path
import struct
import subprocess
import numpy as np
import run

def build():
    sh = run.HERE/'c7feccae4e01e454-safe.metal'
    source=sh.read_text()
    source=source.replace('kernel void main0(', 'kernel void main0(device uint4* audit [[buffer(7)]], ')
    marker='    uint _405 ='
    row='gl_WorkGroupID.x*32u + gl_LocalInvocationID.y*8u + gl_LocalInvocationID.x'
    source=source.replace(marker,f'    uint laneAudit={row};\n    audit[laneAudit]=uint4(_379,_403,_402,1u);\n'+marker)
    assert source.count('            return;')==1
    source=source.replace('            return;', '            audit[laneAudit].w=2u;\n            return;')
    for mark, status in [('if (_15424)',3),('if (_15434)',4)]:
        # Preserve control structure and record the actual branch when it exits.
        assert source.count(mark)==1
        source=source.replace(mark,f'audit[laneAudit].w={mark[3:]} ? {status}u : {status+10}u;\n    '+mark)
    shader=run.HERE/'trace.metal'
    shader.write_text(source)
    source=(run.OLD/'opensource-replay.m').read_text()
    source=source.replace('[enc setComputePipelineState:pso];','id<MTLBuffer>audit=[dev newBufferWithLength:1020*32*16 options:MTLResourceStorageModeShared]; [enc setBuffer:audit offset:0 atIndex:7]; [enc setComputePipelineState:pso];')
    source=source.replace('printf("groups=', '[[NSData dataWithBytes:audit.contents length:audit.length]writeToFile:[out stringByAppendingPathComponent:@"audit.bin"] atomically:YES];\nprintf("groups=')
    host=run.HERE/'trace.m'
    host.write_text(source)
    exe=run.HERE/'trace'
    subprocess.run(['xcrun','clang','-fobjc-arc','-O2','-framework','Metal','-framework','Foundation',host,'-o',exe],check=True)
    return exe,shader

def main():
    exe,shader=build()
    results={}
    for label in ('c7a','c7b'):
        manifest=run.SAMPLES[label][1]
        cap=Path(json.loads(manifest.read_text())['directory'])
        tiles=np.fromfile(cap/'before-Srv-s1-d8.bin',dtype='<u4')[:1020]
        rows=[]
        for i in range(20):
            out=run.HERE/('trace-'+label)
            with (run.HERE/('trace-'+label+'.log')).open('w' if i==0 else 'a') as f:
                subprocess.run([exe,manifest,shader,out],env={**run.ENVS,'REPLAY_PRECISE':'1'},check=True,stdout=f,stderr=subprocess.STDOUT)
            diffs=run.compare(manifest,out)
            audit=np.fromfile(out/'audit.bin',dtype='<u4').reshape(1020,32,4)
            lane=np.arange(32,dtype='uint32')[None,:]
            xs=(tiles[:,None]&255)*8+lane%8
            ys=((tiles[:,None]>>8)&127)*4+lane//8
            done=audit[:,:,3]
            row={'exact':diffs['exact'],'bytes':diffs['mismatching_bytes'],'zero_lanes':int((done==0).sum()),'unfinished_lanes':int((done!=2).sum()),'statuses':{str(v):int((done==v).sum()) for v in np.unique(done)},'wrong_input_lanes':int((audit[:,:,0]!=tiles[:,None]).sum()),'wrong_coords':int(((audit[:,:,1]!=xs)|(audit[:,:,2]!=ys)).sum())}
            rows.append(row)
            if row['exact']!=10: break
        results[label]=rows
        print(label,rows[-1],len(rows),flush=True)
    (run.HERE/'trace.json').write_text(json.dumps(results,indent=2)+'\n')
if __name__=='__main__':
    main()
