"""Copy of first-scene-latency/run_scene.py for the masked-store / scoped-tracking A/B on picture-textures.
Only differences: --extra adds VICTORY_* settings to the candidate arm, captures follow
 tmp/submit-perf-9njeneg8/frame-times.py anchors (+8,+16,+24,...), runs land in this folder."""
import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import time

import sys as _sys
_sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "first-scene-latency"))
from window_observer import observe  # noqa: E402

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
import sys; sys.path.insert(0, str(ROOT / "first-scene-latency"))
INSTALLED = ROOT / "victory-gptk4-run/picture-textures"
PUBLIC = ROOT / "启动 GTA VI 第一幕画面合成.command"
MATERIAL = ROOT / "启动 GTA VI 第一幕材质渲染.command"


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise ValueError(f"The original launcher has changed at {old!r}")
    return text.replace(old, new)


def launchers(folder):
    material = folder / "material.command"
    script = replace_once(MATERIAL.read_text(), 'workspace="${0:A:h}"',
                          "workspace=" + shlex.quote(str(ROOT)))
    material.write_text(replace_once(script, 'exec /opt/homebrew/bin/python3 "$workspace/picture_shader_session.py"',
                                     'exec /opt/homebrew/bin/python3 "$LATENCY_SESSION_SCRIPT"'))
    wrapper = folder / "picture.command"
    wrapper.write_text(replace_once(PUBLIC.read_text(), 'exec "${0:A:h}/启动 GTA VI 第一幕材质渲染.command"',
                                    "exec " + shlex.quote(str(material))))
    for executable in (material, wrapper):
        executable.chmod(0o755)
    return wrapper


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(args):
    binary = args.binary.resolve(strict=True)
    (HERE / "runs").mkdir(exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix=args.name + "-", dir=HERE / "runs"))
    print(folder, flush=True)
    wrapper = launchers(folder)
    env = {key: value for key, value in os.environ.items() if not key.startswith("VICTORY_")}
    settings = dict(
        VICTORY_TEST_SECONDS=str(args.seconds), VICTORY_CAPTURE_FRAMES="", VICTORY_CAPTURE_NATIVE="1",
        VICTORY_LATENCY_MODE=args.mode, VICTORY_LATENCY_SUBJECT=args.subject,
        VICTORY_XMEM_VIEW_READ=str(int(args.xmem)), VICTORY_MERGE_WRITE_SNAPSHOTS=str(int(args.snapshots)),
        VICTORY_MATERIAL_INPUTS="0" if args.light_trace else "1",
        # The world detector needs a nonempty list. Keep captures outside the light-trace window.
        VICTORY_CAPTURE_WORLD="8,16,24,30,40,48",
    )
    for item in args.extra:
        key, _, value = item.partition("=")
        settings[key] = Path(value[1:]).read_text().split() and ",".join(Path(value[1:]).read_text().split()) if value.startswith("@") else value
    env.update(settings)
    with (ROOT / "tmp/submit-perf-9njeneg8/run.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        while subprocess.run(["/usr/bin/pgrep", "-qf", r"^C:.*victory\.exe run --execute"]).returncode == 0:
            time.sleep(2)
        with tempfile.TemporaryDirectory(dir=INSTALLED.parent, prefix="latency-bin-") as scratch:
            install = Path(scratch)
            (install / "victory.exe").symlink_to(binary)
            for dependency in [*INSTALLED.glob("*.dll"), INSTALLED / "victory-metal-daemon"]:
                (install / dependency.name).symlink_to(dependency.resolve())
            session = folder / "session.py"
            script = replace_once((ROOT / "picture_shader_session.py").read_text(),
                                  "root = Path(__file__).resolve().parent", f"root = Path({str(ROOT)!r})")
            session.write_text(replace_once(script, "install = install.with_name('picture-' + edition)",
                                            f"install = Path({str(install)!r})"))
            env["LATENCY_SESSION_SCRIPT"] = str(session)
            meta = dict(exe=str(binary), sha=digest(binary), daemon_sha=digest(INSTALLED / "victory-metal-daemon"),
                        public_launcher_sha=digest(PUBLIC), mode=args.mode, settings=settings,
                        start_utc=datetime.now(timezone.utc).isoformat())
            output = folder / "run.json"
            output.write_text(json.dumps(meta, indent=2))
            driver_log = folder / "driver.log"
            with driver_log.open("w") as log:
                process = subprocess.Popen([wrapper], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
                try:
                    observe(folder, ROOT, process)
                except (OSError, ValueError, subprocess.SubprocessError) as exc:
                    (folder / "observer-error.txt").write_text(str(exc))
                meta["exit"] = process.wait()
            match = re.search(r"logging to (.+?\.log)", driver_log.read_text())
            if match:
                meta["log"] = match[1]
                print(match[1], flush=True)
            output.write_text(json.dumps(meta, indent=2) + "\n")
            return 0 if match and meta["exit"] == 0 else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("name")
    parser.add_argument("binary", type=Path)
    parser.add_argument("--seconds", type=int, default=230)
    parser.add_argument("--mode", choices=["off", "on", "abba", "baab"], default="off")
    parser.add_argument("--subject", choices=["xmem", "snapshots"], default="xmem")
    parser.add_argument("--xmem", action="store_true")
    parser.add_argument("--snapshots", action="store_true")
    parser.add_argument("--extra", action="append", default=[], help="KEY=VALUE or KEY=@file (whitespace list joined by commas)")
    parser.add_argument("--light-trace", action="store_true",
                        help="Keep diagnostic image exports outside the measured interval.")
    raise SystemExit(run(parser.parse_args()))
