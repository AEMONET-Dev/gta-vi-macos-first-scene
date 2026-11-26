from pathlib import Path
import argparse, os, shutil, subprocess, sys
here = Path(__file__).resolve().parent
root = here.parents[1]
p = argparse.ArgumentParser()
p.add_argument('name')
p.add_argument('--scoped',choices=['0','1'],default='1')
p.add_argument('--merge',choices=['0','1'],default='1')
p.add_argument('--fxc',action='store_true')
p.add_argument('--compat',action='store_true')
args=p.parse_args()
outdir=here/('native-fxc' if args.fxc else 'system-fxc')
if args.compat: outdir=here/'mac-compat-tests'
outdir.mkdir(exist_ok=True)
exe=next((here/('compat-target' if args.compat else 'build')/'x86_64-pc-windows-gnullvm/release/deps').glob('victory_hle-*.exe' if args.name.startswith('modules::') else 'victory_gpu-*.exe'))
if args.compat or not (outdir/'device_check.exe').exists() or exe.stat().st_mtime>(outdir/'device_check.exe').stat().st_mtime:
    shutil.copy2(exe,outdir/'device_check.exe')
for dll in ['dxil.dll','dxcompiler.dll','libunwind.dll']:
    target=outdir/dll
    if not target.exists():
        target.symlink_to((root/'victory-gptk4-run'/dll).resolve(strict=True))
if args.fxc:
    path = os.getenv('FXC_DLL')
    if not path:
        available=sorted((Path.home()/'Library/Application Support/CrossOver/Bottles').glob('*/drive_c/Program Files (x86)/Steam/bin/cef/cef.win64/d3dcompiler_47.dll'))
        path=available[0] if available else None
    if not path: raise SystemExit('Set FXC_DLL to an existing x64 D3DCompiler_47 DLL')
    dll=outdir/'d3dcompiler_47.dll'
    if not dll.exists():
        dll.symlink_to(Path(path).resolve(strict=True))
env=dict(os.environ,CX_BOTTLE_PATH=str(root/'bottles'),TMPDIR=str(root/'tmp'),ROSETTA_ADVERTISE_AVX='1',WINEDEBUG='-all',VICTORY_CLAMP_SCOPED=args.scoped,VICTORY_BARRIER_MERGE=args.merge,VICTORY_RELAY='0',WINEDLLOVERRIDES='mscoree,mshtml=;winedbg.exe=d;d3d12,d3d12core,dxgi=b'+(';d3dcompiler_47=n' if args.fxc else ''))
if args.compat: env['VICTORY_FAULT_TRACKING']='0'
win='Z:'+str(outdir).replace('/',chr(92))
cmd=['/usr/bin/sandbox-exec','-f',str(root/'test.sb'),str(root/'runtime-4/bin/wine'),'--bottle','GTA-VI-GPTK4','--no-update','--no-gui','--workdir',win,'--cx-app',win+chr(92)+'device_check.exe','--',(args.name if '::' in args.name else 'indirect::tests::'+args.name),'--exact','--nocapture','--test-threads=1']
r=subprocess.run(cmd,env=env,cwd=outdir,capture_output=True,timeout=80)
log=r.stdout+r.stderr
sys.stdout.buffer.write(log)
if r.returncode or b'1 passed; 0 failed' not in log or not (b'BENCH' in log or b'hardware ' in log or b'GROUPS' in log or b'FULL_COPY_OK' in log or args.name.startswith('modules::')): raise SystemExit(r.returncode or 1)
