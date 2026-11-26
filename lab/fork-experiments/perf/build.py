from pathlib import Path
import os, subprocess, sys
here = Path(__file__).resolve().parent
root = here.parents[1]
mingw = next((root/"toolchain").glob("llvm-mingw-*-macos-universal"))/"bin"
env = dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1", CARGO_HOME=str(root/"toolchain/cargo"), RUSTUP_HOME=str(root/"toolchain/rustup"), CARGO_TARGET_DIR=str(here/('compat-target' if '--compat' in sys.argv else 'build')), CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=str(mingw/"x86_64-w64-mingw32-clang"), PATH=f"{root}/toolchain/cargo/bin:{mingw}:{os.environ['PATH']}", CC_x86_64_pc_windows_gnullvm=str(mingw/"x86_64-w64-mingw32-clang"), AR_x86_64_pc_windows_gnullvm=str(mingw/"llvm-ar"))
game = "--launcher" in sys.argv[1:]
args = ["build","-p","victory","--features","diagnostics"] if game else ["test","-p","victory-hle" if "--hle" in sys.argv else "victory-gpu","--no-run"]
work = here.parent/"repos/victory-perf" if game else here/"src"
if "--compat" in sys.argv: work = here/"compat-src"
raise SystemExit(subprocess.call(["cargo",*args,"--release","--locked","--target","x86_64-pc-windows-gnullvm"],env=env,cwd=work))
