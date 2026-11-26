"""Capture only our test's on-screen window, including the final overlay."""
import datetime,json,subprocess,sys
from pathlib import Path
run=Path(sys.argv[1]).resolve()
here=Path(__file__).resolve().parent
program=here/"window_frames"
if not program.exists():
    subprocess.run(["swiftc",here/"window_frames.swift","-o",program],check=True)
meta=json.loads((run/'backend.json').read_text())
windows=[]
for pid in meta['pids']:
    exe=subprocess.run(['ps','-p',pid,'-o','comm='],text=True,capture_output=True).stdout.strip()
    if exe.lower() != ('C:\\mac-fork-'+run.name+'\\victory.exe').lower():
        continue
    for line in subprocess.check_output([program,pid],text=True).splitlines():
        n,layer,width,height,onscreen=line.split()
        if layer=='0' and onscreen in ('1','true') and int(width)>0:
            windows.append((int(width)*int(height),n))
if not windows:
    raise SystemExit('isolated game has no on-screen window')
window=max(windows)[1]
now=datetime.datetime.now(datetime.timezone.utc).strftime('%H%M%S')
png=run/('presented-'+now+'.png')
subprocess.run(['/usr/sbin/screencapture','-x','-o','-l',window,png],check=True)
subprocess.run(['sips','-Z','480',png,'--out',run/('preview-'+now+'.png')],check=True,stdout=subprocess.DEVNULL)
print(png)
