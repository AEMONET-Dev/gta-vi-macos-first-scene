"""Build the clean Mac fork as a Windows executable for the local CrossOver/GPTK runtime."""
from pathlib import Path
import os
import subprocess

experiment = Path(__file__).resolve().parent
root = experiment.parents[1]
repo = experiment.parent / 'repos' / 'VIctory-macOS'
mingw = next((root / 'toolchain').glob('llvm-mingw-*-macos-universal')) / 'bin'
env = dict(os.environ, RUSTUP_TOOLCHAIN='1.98.1', CARGO_HOME=str(root / 'toolchain/cargo'),
           RUSTUP_HOME=str(root / 'toolchain/rustup'), CARGO_TARGET_DIR=str(experiment / 'target'),
           PATH=os.pathsep.join((str(root / 'toolchain/cargo/bin'), str(mingw), os.environ['PATH'])),
           CC_x86_64_pc_windows_gnullvm=str(mingw / 'x86_64-w64-mingw32-clang'),
           AR_x86_64_pc_windows_gnullvm=str(mingw / 'llvm-ar'),
           CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=str(mingw / 'x86_64-w64-mingw32-clang'))
subprocess.run(['cargo', 'build', '--release', '--locked', '--target', 'x86_64-pc-windows-gnullvm',
                '-p', 'victory', '--features', 'diagnostics'], env=env, cwd=repo, check=True)
