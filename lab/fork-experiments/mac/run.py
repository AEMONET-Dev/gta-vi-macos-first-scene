"""Run the same release binary in private, separately seeded Wine bottles."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

experiment = Path(__file__).resolve().parent
root = experiment.parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('mode', choices=('default', 'tracking', 'patch-only'))
parser.add_argument('--seconds', type=int, default=185)
parser.add_argument('--binary', type=Path)
parser.add_argument('--source-repo', type=Path, help='Git checkout used to build --binary')
parser.add_argument('--exe-label', help='Short label for this test directory')
parser.add_argument('--mullion-runtime', type=Path, help='Private CrossOver installation with Mullion installed')
parser.add_argument('--fxc-dll', type=Path, help='Native d3dcompiler_47.dll for this bottle')
parser.add_argument('--set', action='append', default=[], metavar='VICTORY_KEY=VALUE')
args = parser.parse_args()
repo = args.source_repo or (None if args.binary else experiment.parent / 'repos/VIctory-macOS')
binary = (args.binary or experiment / 'target/x86_64-pc-windows-gnullvm/release/victory.exe').resolve(strict=True)
runtime = (args.mullion_runtime or root / 'runtime-4').resolve(strict=True)
if not (runtime / 'bin/wine').is_file():
    parser.error(f'Wine launcher is missing from {runtime}')
fxc = args.fxc_dll.resolve(strict=True) if args.fxc_dll else None
label = args.exe_label or args.mode
if not re.fullmatch(r'[a-zA-Z0-9_-]+', label):
    parser.error('--exe-label must contain only letters, digits, underscores, or hyphens')
overrides = {}
for pair in args.set:
    key, separator, value = pair.partition('=')
    if not separator or not re.fullmatch('VICTORY_[A-Z0-9_]+', key):
        parser.error('--set expects VICTORY_KEY=VALUE')
    overrides[key] = value
if args.mullion_runtime:
    label = 'mullion-' + label
run = Path(tempfile.mkdtemp(dir=experiment, prefix=label + '-'))
tag = 'mac-fork-' + run.name
bottles = run / 'bottles'
bottles.mkdir()
bottle = bottles / tag
def clone(source, dest):
    subprocess.run(['/bin/cp', '-cR', str(source), str(dest)], check=True)

clone(root / 'bottles/GTA-VI-GPTK4', bottle)
if args.mullion_runtime:
    conf = bottle / 'cxbottle.conf'
    cfg = re.sub('^"(?:CX_ROOT|CX_GRAPHICS_BACKEND)" = .*\n', '', conf.read_text(), flags=re.M)
    header = '[EnvironmentVariables]\n'
    assert cfg.count(header) == 1
    conf.write_text(cfg.replace(header, header + f'"CX_ROOT" = "{runtime}"\n"CX_GRAPHICS_BACKEND" = "dxmt"\n'))
drive = bottle / 'drive_c'
for alias in ('gta-vi','victory','victory-cache','victory-data','victory-frames','gptk4-device-check'):
    (drive / alias).unlink()
for name in ('victory-gptk4-cache','victory-gptk4-data'):
    short = name.replace('-gptk4', '')
    clone(root / name, drive / short)
assets = drive / 'gta-vi'
assets.mkdir()
for original in (root / 'game').iterdir():
    target = assets / original.name
    if original.is_symlink():
        target.symlink_to(original.resolve())
    elif original.is_dir():
        clone(original, target)
    else:
        target.write_bytes(original.read_bytes())
binfolder = drive / tag
binfolder.mkdir()
clone(binary, binfolder / 'victory.exe')
for dll in (root / 'victory-run').glob('*.dll'):
    (binfolder / dll.name).symlink_to(dll.resolve())
if fxc:
    local_fxc = binfolder / 'd3dcompiler_47.dll'
    local_fxc.unlink(missing_ok=True)
    local_fxc.symlink_to(fxc)
frames = run / 'frames'
frames.mkdir()
(drive / 'victory-frames').symlink_to(frames)
temp = run / 'tmp'
temp.mkdir()
env = {k: v for k, v in os.environ.items() if not k.startswith('VICTORY_')}
settings = dict(VICTORY_GAME_DIR=r'C:\gta-vi', VICTORY_DATA_DIR=r'C:\victory-data',
                VICTORY_CACHE_DIR=r'C:\victory-cache', VICTORY_LOG='info',
                VICTORY_CAPTURE=r'C:\victory-frames', VICTORY_CAPTURE_FRAMES='',
                VICTORY_CAPTURE_WORLD='2,3,4,5,6,7,8,9,10,11,12',
                VICTORY_TEST_WINDOW='1', VICTORY_WINDOW_MAX='1280x720',
                VICTORY_KEYS=','.join(f'{s}:Enter' for s in range(10, 110, 5)))
if args.mode in ('tracking', 'patch-only'):
    settings['VICTORY_PATCH_MASKED'] = 'all'
    settings['VICTORY_FAULT_TRACKING'] = '1' if args.mode == 'tracking' else '0'
settings.update(overrides)
dll_order = 'mscoree,mshtml=;winedbg.exe=d;d3d12,d3d12core,dxgi'
if args.mullion_runtime:
    dll_order += ',winemetal'
    mullion_cache = run / 'mullion-cache'
    mullion_cache.mkdir()
    env.update(CX_ROOT=str(runtime), CX_GRAPHICS_BACKEND='dxmt',
               MULLION_USE_DEFAULT_METAL_CACHE='1', MULLION_SHADER_CACHE_PATH=str(mullion_cache))
dll_order += '=b'
if fxc:
    dll_order += ';d3dcompiler_47=n'
env.update(settings, CX_BOTTLE_PATH=str(bottles), TMPDIR=str(temp), ROSETTA_ADVERTISE_AVX='1', WINEDEBUG='-all',
           WINEDLLOVERRIDES=dll_order)
base = 'C:\\' + tag
exe = base + '\\victory.exe'
cmd = ['/usr/bin/sandbox-exec','-f',str(root / 'test.sb'),str(runtime / 'bin/wine'),
       '--bottle',tag,'--no-update','--no-gui','--workdir',base,'--cx-app',exe,'--',
       'run','--execute','--console','series-s',r'C:\gta-vi\game_scarlett_master.exe']
metadata = dict(commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip() if repo else None,
                source_repo=str(repo.resolve()) if repo else None, binary=str(binary),runtime=str(runtime),
                backend='Mullion' if args.mullion_runtime else 'D3DMetal',fxc_dll=str(fxc) if fxc else None,
                mullion_shader_cache=env.get('MULLION_SHADER_CACHE_PATH') if args.mullion_runtime else None,
                mullion_use_default_metal_cache=env.get('MULLION_USE_DEFAULT_METAL_CACHE') if args.mullion_runtime else None,
                fxc_sha256=hashlib.file_digest(fxc.open('rb'),'sha256').hexdigest() if fxc else None,
                dll_overrides=dll_order, label=label,
                binary_sha256=hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),
                mode=args.mode,settings=settings, deadline_seconds=args.seconds,
                started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
def write_meta():
    (run / 'run.json').write_text(json.dumps(metadata,indent=2)+'\n')
write_meta()
print(run, flush=True)
with (run / 'game.log').open('wb') as output:
    proc = subprocess.Popen(cmd, env=env, cwd=binfolder, stdout=output, stderr=subprocess.STDOUT,start_new_session=True)
    start = time.monotonic()
    log = run / 'game.log'
    try:
        while proc.poll() is None:
            elapsed = time.monotonic() - start
            if elapsed >= args.seconds:
                metadata['stop'] = 'time-limit'
                break
            log_data = log.read_bytes()
            if b'rosetta error:' in log_data:
                metadata['stop'] = 'Rosetta reported a fatal translation error'
                break
            match = re.search(rb"world's first frame[^\n]*?frame\x1b[^\n]*?=(?:\x1b\[[0-9;]*m)*(\d+)|world's first frame[^\n]*?frame=(\d+)", log_data)
            if match:
                world = int(next(x for x in match.groups() if x))
                if (frames / f'frame-{world+12}.png').exists():
                    time.sleep(2)
                    metadata['stop'] = 'captured world +12'
                    break
            time.sleep(1)
        if 'stop' not in metadata:
            metadata['stop'] = 'process exited'
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid,signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid,signal.SIGKILL)
                proc.wait()
        for row in subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines():
            fields = row.strip().split(None,1)
            if len(fields)==2 and fields[1].casefold()==exe.casefold():
                try:
                    os.kill(int(fields[0]),signal.SIGTERM)
                except ProcessLookupError:
                    pass
        metadata.update(elapsed_s=round(time.monotonic()-start,3),returncode=proc.returncode)
        write_meta()
