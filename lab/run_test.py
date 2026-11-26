from pathlib import Path
import argparse
import os
import re
import signal
import subprocess
import sys

work = Path(__file__).resolve().parent
cli = Path('/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin')
p = argparse.ArgumentParser()
p.add_argument('target', choices=['victory','xcl'])
p.add_argument('step', choices=['config','run'])
p.add_argument('--seconds', type=int, default=250)
p.add_argument('--console', choices=['series-s','series-x'], default='series-s')
p.add_argument('--label', default='')
p.add_argument('--victory-dir', default='victory')
p.add_argument('--test-window', action='store_true')
p.add_argument('--capture', action='store_true')
p.add_argument('--gptk4', action='store_true')
args = p.parse_args()
bottle = 'GTA-VI-Test'
if args.gptk4:
    cli = work/'runtime-4/bin'
    bottle = 'GTA-VI-GPTK4'
env = dict(os.environ, CX_BOTTLE_PATH=str(work/'bottles'), TMPDIR=str(work/'tmp'),
           ROSETTA_ADVERTISE_AVX='1', WINEDEBUG='-all')
def darwin_directory(key):
    return str(Path(subprocess.check_output(['/usr/bin/getconf', key], text=True).strip()).resolve())

for folder in ('logs','tmp'):
    (work / folder).mkdir(exist_ok=True)
prefix = ['/usr/bin/sandbox-exec', '-D', f'WORKSPACE={work}', '-D', f'WINE_TMP=/private/tmp/.wine-{os.getuid()}',
          '-D', 'DARWIN_CACHE=' + darwin_directory('DARWIN_USER_CACHE_DIR'),
          '-D', 'DARWIN_TEMP=' + darwin_directory('DARWIN_USER_TEMP_DIR'),
          '-D', 'TEMP_ITEM_REGEX=^' + re.escape(darwin_directory('DARWIN_USER_TEMP_DIR')) + '/TemporaryItems/NSIRD_(wine|wineloader|device_check|victory)[^/]*(/|$)',
          '-f', str(work/'test.sb'), str(cli/'wine'),
          '--bottle',bottle,'--no-update','--no-gui']
if args.target == 'victory':
    if args.test_window:
        env.update(VICTORY_TEST_WINDOW='1', VICTORY_WINDOW_MAX='1280x720')
    base = 'C:\\' + args.victory_dir
    exe = base + '\\victory.exe'
    env.update(VICTORY_GAME_DIR=r'C:\gta-vi', VICTORY_DATA_DIR=r'C:\victory-data',
               VICTORY_CACHE_DIR=r'C:\victory-cache', VICTORY_LOG='info',
               WINEDLLOVERRIDES='mscoree,mshtml=;winedbg.exe=d;d3d12,d3d12core,dxgi=b')
    frame_root = work/'victory-frames'/(args.label or 'default')
    frame_root.mkdir(exist_ok=True)
    if args.seconds > 0 or args.capture:
        env['VICTORY_CAPTURE'] = 'C:\\victory-frames\\' + frame_root.name
        env.setdefault('VICTORY_CAPTURE_FRAMES', '128,512')
    command = ['config'] if args.step == 'config' else ['run','--execute','--console',args.console,r'C:\gta-vi\game_scarlett_master.exe']
else:
    base, exe = r'C:\xcl', r'C:\xcl\xcl.exe'
    env.update(XCL_DXC_DIR=base, XCL_NETWORK='0', XCL_FRAME_DUMP='180', XCL_FRAME_DUMP_MAX='8')
    command = ['run',r'C:\gta-vi\game_scarlett_master.exe']
log = work/'logs'/('-'.join([args.target,args.step,args.label or 'default'])+'.log')
with log.open('wb') as out:
    proc = subprocess.Popen(prefix+['--workdir',base,'--cx-app',exe,'--']+command,
                            env=env,cwd=work,stdout=out,stderr=subprocess.STDOUT,start_new_session=True)
    print(f'PID {proc.pid} logging to {log}',flush=True)
    try:
        result = proc.wait(timeout=args.seconds or None)
        print(f'Exit code: {result}', flush=True)
        sys.exit(result)
    except subprocess.TimeoutExpired:
        print('Reached test duration',flush=True)
        os.killpg(proc.pid,signal.SIGTERM)
        proc.wait(timeout=10)
