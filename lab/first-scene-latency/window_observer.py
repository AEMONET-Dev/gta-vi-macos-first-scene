"""Capture this run's presented window following selected in-game frame anchors."""
from pathlib import Path
import json
import re
import subprocess
import time


def observe(directory: Path, root: Path, process):
    helper = root / "fork-experiments/window_frames"
    driver = directory / "driver.log"
    path = None
    while process.poll() is None and path is None:
        match = re.search(r"logging to (.+?\.log)", driver.read_text(errors="replace"))
        if match:
            path = Path(match[1])
        time.sleep(0.25)
    if path is None:
        return
    label = path.stem.removeprefix("victory-run-")
    suffix = label.split("-", 3)[-1]
    expected = "C:\\victory\\picture-" + suffix + "\\victory.exe"
    world = None
    seen = set()
    records = []
    pid = None
    with path.open(errors="replace") as stream:
        while process.poll() is None:
            for line in stream:
                match = re.search(r"world's first frame.* frame=(\d+)", line)
                if match:
                    world = int(match[1])
                match = re.search(r"frame saved.*?frame-(\d+)\.png", line)
                if not match and "frame delivered" in line:
                    match = re.search(r"frame=(\d+)", line)
                if world is None or not match:
                    continue
                frame = int(match[1])
                if frame in seen or frame - world not in (16, 24, 32, 40):
                    continue
                if pid is None:
                    for row in subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True).splitlines():
                        fields = row.strip().split(None, 1)
                        if len(fields) == 2 and fields[1].casefold() == expected.casefold():
                            pid = fields[0]
                            break
                if not pid:
                    continue
                windows = []
                for row in subprocess.check_output([helper, pid], text=True).splitlines():
                    wid, layer, width, height, visible = row.split()
                    if layer == "0" and visible in ("1","true"):
                        windows.append((int(width)*int(height),wid))
                if windows:
                    seen.add(frame)
                    window = max(windows)[1]
                    output = directory / f"window-after-{frame}.png"
                    result = subprocess.run(["/usr/sbin/screencapture","-x","-o","-l",window,output])
                    records.append(dict(pid=pid, window=window, anchor=frame, world_offset=frame-world,
                                        returncode=result.returncode, file=output.name))
                    (directory / "windows.json").write_text(json.dumps(records,indent=2)+"\n")
            time.sleep(0.12)
