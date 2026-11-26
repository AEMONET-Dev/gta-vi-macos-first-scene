"""Record the graphics files actually mapped by an isolated experiment process."""
import argparse
import json
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('run', type=Path)
parser.add_argument('--seconds', type=int, default=35)
args = parser.parse_args()
run = args.run.resolve()
exe_suffix = (run.name + '\\victory.exe').lower()
deadline = time.monotonic() + args.seconds
observed = set()
pids = set()
while time.monotonic() < deadline:
    for row in subprocess.check_output(['ps','-axo','pid=,comm='],text=True).splitlines():
        pair = row.strip().split(None,1)
        if len(pair) != 2 or not pair[1].lower().endswith(exe_suffix):
            continue
        pid = pair[0]
        pids.add(pid)
        files = subprocess.run(['/usr/sbin/lsof','-Fn','-p',pid],capture_output=True,text=True).stdout
        for path in files.splitlines():
            if path.startswith('n/') and any(x in path.lower() for x in ('d3d12','dxgi.dll','winemetal','d3dmetal','d3dcompiler_47')):
                observed.add(path[1:])
    if any(s.lower().endswith(('d3d12core.dll','/d3dmetal','winemetal.so')) for s in observed):
        break
    time.sleep(.5)
(run/'backend.json').write_text(json.dumps({'pids':sorted(pids),'graphics_files':sorted(observed)},indent=2)+'\n')
print(run.name,len(observed),flush=True)
