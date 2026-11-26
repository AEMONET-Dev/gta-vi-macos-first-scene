"""Own one native translator and one Wine run, using a private mapped file."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def main():
    root = Path(__file__).resolve().parent
    install = root / 'victory-gptk4-run' / 'native-bridge'
    daemon = install / 'victory-metal-daemon'
    wine = install / 'victory.exe'
    if not daemon.is_file() or not wine.is_file():
        raise SystemExit('原生翻译版本尚未编译完成。')
    cache = root / 'victory-native-shader-cache'
    env = dict(os.environ, VICTORY_METAL_CACHE_DIR=str(cache))
    with tempfile.TemporaryDirectory(prefix='bridge-', dir=install.parent) as session:
        folder = Path(session)
        for binary in (wine, *install.glob('*.dll')):
            (folder / binary.name).symlink_to(binary)
        label = 'native-bridge-' + time.strftime('%Y%m%d-%H%M%S') + '-' + folder.name[7:]
        log = root / 'logs' / f'{label}-daemon.log'
        shared = folder / 'shader.shm'
        win_dir = 'victory\\' + folder.name
        env['VICTORY_SHADER_BRIDGE'] = 'C:\\' + win_dir + '\\' + shared.name
        with log.open('wb') as out:
            proc = subprocess.Popen([str(daemon), str(shared)], env=env, stdout=out, stderr=subprocess.STDOUT)
            game = None
            try:
                deadline = time.monotonic() + 15
                while proc.poll() is None and time.monotonic() < deadline:
                    if shared.is_file():
                        with shared.open('rb') as f:
                            if int.from_bytes(f.read(8), 'little') == 0x564943544D544C32:
                                break
                    time.sleep(.05)
                else:
                    raise SystemExit(f'原生翻译服务启动失败。日志 {log}')
                print(f'已启动 ARM64 着色器翻译服务。日志 {log}', flush=True)
                game = subprocess.Popen([sys.executable, str(root / 'run_test.py'), 'victory', 'run',
                    '--gptk4', '--console', 'series-s', '--test-window', '--victory-dir', win_dir,
                    '--seconds', env.get('VICTORY_TEST_SECONDS', '0'), '--label', label], env=env)
                return game.wait()
            finally:
                if game is not None and game.poll() is None:
                    game.terminate()
                    game.wait()
                # Wine's wrapper may exit before its guest on a timed test.
                # Only this run can have this randomly assigned Windows image path.
                target = 'C:\\' + win_dir + '\\victory.exe'
                for row in subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True).splitlines():
                    fields = row.strip().split(None, 1)
                    if len(fields) == 2 and fields[1].casefold() == target.casefold():
                        try:
                            os.kill(int(fields[0]), signal.SIGTERM)
                        except ProcessLookupError:
                            pass
                shared.unlink(missing_ok=True)
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.terminate()
                    proc.wait()


if __name__ == '__main__':
    def interrupted(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)
    sys.exit(main())
