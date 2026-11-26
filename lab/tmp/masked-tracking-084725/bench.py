"""One first-scene run of a given build with the default material launcher's
settings. `baseline` runs the current renderer; `candidate` the test build with
masked stores patched and write tracking on for the 174 MiB buffer only;
`audit` adds the missed-write audit. Never touches the default launcher."""
from pathlib import Path
import fcntl
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

here = Path(__file__).resolve().parent
root = here.parents[1]
mode = sys.argv[1]
seconds = sys.argv[2] if len(sys.argv) > 2 else '305'
binary = here / ('baseline.exe' if mode == 'baseline' else 'candidate.exe')
installed = root / 'victory-gptk4-run' / 'picture'
env = {k: v for k, v in os.environ.items() if not k.startswith('VICTORY_')}
# The default material launcher's settings (启动 GTA VI 第一幕材质渲染.command).
env.update(VICTORY_HEAP_USAGE='1', VICTORY_NO_DISK_CACHE='1', VICTORY_HEAP_ONCE_PER_FRAME='1',
           VICTORY_PIPELINE_WORKERS='6', VICTORY_PRESENT_PLANE='0', VICTORY_SPD_WAVE32='1',
           VICTORY_MATERIAL_LIGHT='1', VICTORY_MATERIAL_EARLY_SCENE='1', VICTORY_SPATIAL_UPSCALE='1',
           VICTORY_MATERIAL_SURFACES='1',
           VICTORY_KEYS=','.join(f'{s}:Enter' for s in range(10, 90, 5)),
           VICTORY_TEST_SECONDS=seconds,
           VICTORY_CAPTURE_WORLD='8,16,24,30,40,48', VICTORY_CAPTURE_FRAMES='')
if mode in ('candidate', 'audit'):
    rvas = (here / 'patch-rvas.txt').read_text().split()
    env.update(VICTORY_PATCH_MASKED=','.join(rvas), VICTORY_TRACK_ONLY_LEN=str(174 << 20))
if mode == 'audit':
    env.update(VICTORY_TRACK_AUDIT='1')
with (root / 'tmp/submit-perf-9njeneg8/run.lock').open('a') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    while True:
        rows = subprocess.check_output(['ps', '-axo', 'comm=,command='], text=True)
        busy = any(re.search(r'^\s*C:', r, re.I) and 'victory.exe' in r.casefold() for r in rows.splitlines())
        if not busy:
            break
        time.sleep(4)
    with tempfile.TemporaryDirectory(prefix='masked-bin-', dir=installed.parent) as scratch:
        out = Path(scratch)
        shutil.copy2(binary, out / 'victory.exe')
        for dep in [*installed.glob('*.dll'), installed / 'victory-metal-daemon']:
            (out / dep.name).symlink_to(dep)
        env['RENDER_BENCH_INSTALL'] = str(out)
        session = root / 'picture_shader_session.py'
        source = session.read_text()
        original = "install = root / 'victory-gptk4-run' / 'picture'"
        assert source.count(original) == 1
        source = source.replace(original, "install = Path(os.environ['RENDER_BENCH_INSTALL'])")
        sha = hashlib.file_digest((out / 'victory.exe').open('rb'), 'sha256').hexdigest()
        print('mode', mode, 'binary SHA-256', sha, 'at', time.strftime('%H:%M:%S'), flush=True)
        os.environ.clear()
        os.environ.update(env)
        exec(compile(source, str(session), 'exec'), {'__file__': str(session), '__name__': '__main__'})
