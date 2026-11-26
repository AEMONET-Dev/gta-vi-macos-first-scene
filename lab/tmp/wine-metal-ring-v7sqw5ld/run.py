from pathlib import Path
import hashlib
import json
import math
import os
import subprocess as sp
import sys
import time

here = Path(__file__).resolve().parent
root = here.parents[1]
cc = next((root / 'toolchain').glob('llvm-mingw-*/bin/x86_64-w64-mingw32-clang'))
sp.run([cc, '-std=c11', '-O2', '-Werror', 'client.c', '-o', 'device_check.exe'], cwd=here, check=True)
sp.run(['/usr/bin/clang', '-O2', '-fobjc-arc', '-framework', 'Metal', '-framework', 'Foundation', 'server.m', '-o', 'metal_probe'], cwd=here, check=True)
arena = here / 'arena.bin'
arena.unlink(missing_ok=True)
env = dict(os.environ, CX_BOTTLE_PATH=str(root/'bottles'), TMPDIR=str(root/'tmp'), ROSETTA_ADVERTISE_AVX='1', WINEDEBUG='-all', WINEDLLOVERRIDES='mscoree,mshtml=;winedbg.exe=d;d3d12,d3d12core,dxgi=b')
sandbox = ['/usr/bin/sandbox-exec', '-f', str(root/'test.sb')]
with (here/'server.log').open('w') as log:
    srv = sp.Popen([*sandbox, here/'metal_probe', arena], cwd=here, env=env, stdout=log, stderr=sp.STDOUT)
    try:
        limit = time.monotonic() + 20
        while b'ready gpu' not in (here/'server.log').read_bytes():
            if srv.poll() is not None or time.monotonic() > limit:
                raise RuntimeError((here/'server.log').read_text())
            time.sleep(.02)
        win = 'Z:' + str(here).replace('/', chr(92))
        cmd = [*sandbox, root/'runtime-4/bin/wine', '--bottle', 'GTA-VI-GPTK4', '--no-update', '--no-gui', '--workdir', win, '--cx-app',win+chr(92)+'device_check.exe', '--']
        with (here/'client.log').open('w') as out:
            proc = sp.run(cmd, cwd=here, env=env, stdout=out, stderr=sp.STDOUT, timeout=60)
        if proc.returncode:
            raise RuntimeError((here/'client.log').read_text())
        if srv.wait(timeout=10):
            raise RuntimeError((here/'server.log').read_text())
    finally:
        if srv.poll() is None: srv.terminate(); srv.wait()
        arena.unlink(missing_ok=True)
raw = (here/'client.log').read_text().splitlines()
rows = [[float(x) for x in r.split(',')] for r in raw if r.split(',')[0].isdigit()]
assert [int(row[0]) for row in rows] == list(range(1,91))
assert any('verified_words_per_round=4194304 rounds=90' in l for l in raw)
stats = {'warmups':10,'measured':80,'bytes_touched_each_round':16777216,'full_verification_per_round':True}
for i, key in enumerate(['roundtrip_ms','gpu_ms','fill_and_roundtrip_and_verify_ms'],1):
    vals = sorted(row[i] for row in rows[10:])
    stats[key] = {'p50':vals[math.ceil(.5*len(vals))-1], 'p95':vals[math.ceil(.95*len(vals))-1], 'max':max(vals)}
stats['empty_IPC_spin_strategy'] = 'idle server sched_yield, waiting Win32 client SwitchToThread'
(here/'results.json').write_text(json.dumps(stats, indent=2)+'\n')
print(json.dumps(stats, indent=2))

