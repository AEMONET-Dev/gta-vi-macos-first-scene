"""Run a released Mullion in a private copy of the project's Wine installation."""
from pathlib import Path
import hashlib
import os
import re
import shutil
import subprocess
import sys

here = Path(__file__).resolve().parent
project = here.parents[1]
release = here / "v0.5-alpha.4"
runtime = here / "runtime"
bottles = here / "bottles"
bottle = bottles / "Mullion"
sdk = next((project / "toolchain").glob("llvm-mingw-*"))
archive = here / "mullion-v0.5-alpha.4.tar.gz"
if hashlib.sha256(archive.read_bytes()).hexdigest() != "ba0faace40544b3c9a02e1f1e5ff8d2dfef2ddf82297426dba2b72a342603447":
    raise SystemExit("release checksum mismatch")
bottles.mkdir(exist_ok=True)
for source, out in [(project / "runtime-4", runtime), (project / "bottles/GTA-VI-Test", bottle)]:
    if not out.exists():
        subprocess.run(["cp", "-cR", source, out], check=True)
for arch in ["x86_64-unix", "x86_64-windows"]:
    dst = runtime / "lib/dxmt" / arch
    for source in (release / arch).iterdir():
        out = dst / source.name
        if out.is_symlink():
            out.unlink()
        shutil.copy2(source, out)
conf = bottle / "cxbottle.conf"
settings = re.sub(r'^"(?:CX_ROOT|CX_GRAPHICS_BACKEND)" = .*\n', "", conf.read_text(), flags=re.M)
settings = settings.replace("[EnvironmentVariables]\n", f'[EnvironmentVariables]\n"CX_ROOT" = "{runtime}"\n"CX_GRAPHICS_BACKEND" = "dxmt"\n')
conf.write_text(settings)
win = "Z:" + str(here).replace("/", "\\")
env = dict(os.environ, CX_ROOT=str(runtime), CX_BOTTLE_PATH=str(bottles),
           CX_GRAPHICS_BACKEND="dxmt", TMPDIR=str(here), ROSETTA_ADVERTISE_AVX="1",
           WINEDLLOVERRIDES="mscoree,mshtml=;winedbg.exe=d;d3d12,d3d12core,dxgi,winemetal=b",
           WINEDEBUG="-all,+loaddll", MULLION_LOG_PATH=win)
cpp = here / "mullion-release/tests/d3d12/d3d12_heap_import.cpp"
tests = {"capability": project/"tmp/hot-backing-QKt99quz/probe.c", "upstream-heap": cpp}
if (here/"wrap.cpp").is_file():
    tests["wrap"] = here/"wrap.cpp"
for name in sys.argv[1:] or tests:
    src = tests[name]
    cxx = src.suffix == ".cpp"
    compiler = sdk / "bin" / ("x86_64-w64-mingw32-clang++" if cxx else "x86_64-w64-mingw32-clang")
    binary = here / (name + ".exe")
    flags = ["-std=c++17","-static","-I",str(cpp.parent)] if cxx else []
    subprocess.run([compiler, "-O2", *flags, src, "-ld3d12", "-ldxguid", "-ld3dcompiler", "-lole32", "-o",binary],check=True)
    command = ["/usr/bin/sandbox-exec", "-f",str(project/"test.sb"), str(runtime/"bin/wine"), "--bottle","Mullion",
               "--no-update","--no-gui","--workdir",win,"--cx-app",win+"\\"+binary.name,"--"]
    log = here / (name + ".log")
    with log.open("w") as output:
        result = subprocess.run(command, env=env, cwd=here, timeout=60, stdout=output, stderr=subprocess.STDOUT)
    print(f"{name} exit {result.returncode}", flush=True)
    for line in log.read_text(errors="replace").splitlines():
        if "trace:loaddll" not in line and line.strip():
            print(line[:200])
    if result.returncode:
        raise SystemExit(result.returncode)
