"""Build real game translations and compare all GPU outputs against the original captures."""
import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess

import numpy as np

HERE = Path(__file__).resolve().parent
BASE = HERE.parents[1]
OLD = BASE / 'tmp/opensource-shader-5jwAJ74p'
ORIGINALS = BASE / 'tmp/native-two-compute-g7ip1avi'
DXIL = HERE / 'build-dxil/dxil-spirv'
VALIDATOR = HERE / 'build-dxil/third_party/SPIRV-Tools/tools/spirv-val'
CROSS = HERE / 'build-cross/spirv-cross'
SAMPLES = {
    'c7a': ('c7feccae4e01e454', BASE / 'tmp/actual-dispatch-b8_w769p/c7-0.json'),
    'c7b': ('c7feccae4e01e454', BASE / 'tmp/actual-dispatch-b8_w769p/c7-1.json'),
    '8bb': ('8bbcbda6008eaa37', BASE / 'tmp/target8-nonzero-56kkk3u3/input.json'),
}
CROSS_FLAGS = '--msl --msl-version 30200 --msl-argument-buffers --msl-argument-buffer-tier 1 --msl-device-argument-buffer 0 --msl-device-argument-buffer 2 --msl-device-argument-buffer 5 --msl-texture-buffer-native'.split()
VARIANTS = {
    'fork': ([], False, False),
    'safe': ([], True, False),
    'force': (['--force-precise'], True, False),
    'arithmetic': ([], True, True),
}
ENVS = {k:v for k,v in os.environ.items() if not k.startswith('REPLAY_')}
def run(command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def compare(manifest, output):
    data = json.loads(manifest.read_text())
    capture = Path(data['directory'])
    by_resource = {}
    for item in data['resources']:
        if item['kind']!='Uav':
            continue
        name = item['name']
        actual_path = output / (name+'.bin')
        expected_path = capture / ('after-'+name+'.bin')
        actual = np.fromfile(actual_path,dtype='uint8')
        expected = np.fromfile(expected_path,dtype='uint8')
        assert actual.size==expected.size
        row={'bytes':int(actual.size),'mismatching_bytes':int(np.count_nonzero(actual!=expected)),
             'expected_sha256':sha(expected_path),'actual_sha256':sha(actual_path)}
        if item['words'][0] in (10,54):
            channels = 4 if item['words'][0]==10 else 1
            a=actual.view('<u2').reshape(-1,channels)
            b=expected.view('<u2').reshape(-1,channels)
            delta=a.astype('int32')-b.astype('int32')
            changed=delta!=0
            order = lambda x:np.where(x&0x8000,0x8000-(x&0x7fff).astype('int32'),0x8000+x.astype('int32'))
            ulps = np.abs(order(a)-order(b))[changed]
            vs=np.abs(a.view('<f2').astype('float32')-b.view('<f2').astype('float32'))[changed]
            row['channels_different']=np.sum(changed,axis=0).tolist()
            row['changed_values']=int(np.count_nonzero(changed))
            row['one_ulp']=int(np.sum(ulps==1))
            row['max_finite_abs_error']=float(np.max(vs[np.isfinite(vs)], initial=0))
        by_resource[name]=row
    return {'exact':sum(item['mismatching_bytes']==0 for item in by_resource.values()),
            'bytes_compared':sum(item['bytes'] for item in by_resource.values()),
            'mismatching_bytes':sum(item['mismatching_bytes'] for item in by_resource.values()),'outputs':by_resource}
def translations(shader,name,dxil_flags,arithmetic):
    spv = HERE / f'{shader}-{name}.spv'
    msl = HERE / f'{shader}-{name}.metal'
    run([DXIL,ORIGINALS / (shader+'.dxil'),'--bindless','--validate',*dxil_flags,'--output',spv])
    run([VALIDATOR,'--target-env','vulkan1.3',spv])
    run([CROSS,spv,*CROSS_FLAGS,'--output',msl])
    if arithmetic:
        code = msl.read_text()
        for before,after in [('return fma(l, r, T(-0.0));','return l * r;'),
                             ('return fma(T(1), l, r);','return l + r;'),
                             ('return fma(T(-1), r, l);','return l - r;')]:
            assert code.count(before)==1
            code=code.replace(before,after)
        msl.write_text(code)
    return msl
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('variants', nargs='*', default=list(VARIANTS))
    args=parser.parse_args()
    run(['xcrun','clang','-fobjc-arc','-O2','-framework','Metal','-framework','Foundation',OLD/'opensource-replay.m','-o',HERE/'replay'])
    results={}
    for variant in args.variants:
        flags,safe,arithmetic=VARIANTS[variant]
        programs={}
        for name,(shader,manifest) in SAMPLES.items():
            if shader not in programs:
                programs[shader]=translations(shader,variant,flags,arithmetic)
            output=HERE / ('outputs-'+variant+'-'+name)
            with (HERE / (variant+'-'+name+'.log')).open('w') as f:
                run([HERE/'replay',manifest,programs[shader],output],env={**ENVS,**({'REPLAY_PRECISE':'1'} if safe else {})},stdout=f,stderr=subprocess.STDOUT)
            result=compare(manifest,output)
            result['metal_sha256']=sha(programs[shader])
            result['spirv_sha256']=sha(programs[shader].with_suffix('.spv'))
            results[variant+'-'+name]=result
            print(variant,name,result['exact'],result['mismatching_bytes'],flush=True)
    (HERE/'results.json').write_text(json.dumps(results,indent=2)+'\n')
if __name__=='__main__':
    main()
