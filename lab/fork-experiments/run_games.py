"""Run the alternative backends one at a time using the same assets and scripted input."""
import json
from pathlib import Path
import subprocess
import sys

here = Path(__file__).resolve().parent
perf = here/"perf/build/x86_64-pc-windows-gnullvm/release/victory.exe"
runner = here/"mac/run.py"
fxc = here/"perf/native-fxc/d3dcompiler_47.dll"
runs = []
out = here/"extra-games.json"
if out.exists():
    runs = json.loads(out.read_text())
for mode in sys.argv[1:]:
    cmd = [sys.executable,"-u",str(runner),"default","--seconds","185"]
    if mode == "mullion":
        cmd += ["--mullion-runtime", str(here/"memory/runtime")]
    elif mode in ("perf-on","perf-off","compat-on","compat-off"):
        flag = "1" if mode.endswith("-on") else "0"
        if mode.startswith("compat-"):
            binary = here/"perf/compat-target/x86_64-pc-windows-gnullvm/release/victory.exe"
            repo = here/"perf/compat-src"
            cmd += ["--set","VICTORY_FAULT_TRACKING=0"]
        else:
            binary, repo = perf, here/"repos/victory-perf"
        cmd += ["--binary",str(binary),"--source-repo",str(repo),"--exe-label",mode,"--fxc-dll",str(fxc),
                "--set","VICTORY_CLAMP_SCOPED="+flag,"--set","VICTORY_BARRIER_MERGE="+flag,
                "--set","VICTORY_CAPTURE_WORLD_ALSO=2,3,4,5,6,7,8,9,10,11,12"]
    else:
        raise ValueError(mode)
    print("starting",mode,flush=True)
    p = subprocess.Popen(cmd, cwd=here/'mac',stdout=subprocess.PIPE,text=True)
    first = p.stdout.readline().strip()
    r = Path(first)
    if not r.is_absolute() or not r.is_dir():
        raise RuntimeError((mode,first,p.wait()))
    print(r,flush=True)
    observer = subprocess.Popen([sys.executable,str(here/"observe_backend.py"),str(r)])
    for line in p.stdout:
        print(line,end='',flush=True)
    rc = p.wait()
    observer.wait()
    if rc:
        raise RuntimeError((mode,rc))
    runs.append(dict(mode=mode,path=str(r.relative_to(here)),argv=cmd[2:]))
    out.write_text(json.dumps(runs,indent=2)+"\n")
    print("completed",mode,flush=True)
