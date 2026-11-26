"""Export separate runtime and regression-test patches for the unmodified fork SHA."""
from pathlib import Path
import difflib, subprocess
here=Path(__file__).resolve().parent
work=here/'compat-src'
sources=[Path('crates/victory-gpu/src')/n for n in ('watch.rs','lib.rs','draw.rs','texture.rs','shadow.rs')]+[Path('crates/victory-hle/src/modules/xmem.rs')]
runtime, tests=[],[]
for path in sources:
    base=subprocess.check_output(['git','show',f'HEAD:{path}'],cwd=work).decode()
    combined=(work/path).read_text()
    active=combined
    if path.name=='shadow.rs':
        active=base
    if path.name=='texture.rs':
        active=active.removesuffix('\n'+(here/'compat-texture-test.rs').read_text())
    if path.name=='xmem.rs':
        active=active.replace('assert_eq!(watched == 0, victory_gpu::fault_tracking());','assert_eq!(watched, 0);')
    def append(dst,old,new):
        if old!=new:
            dst.append(f'diff --git a/{path} b/{path}\n')
            dst.extend(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile=f'a/{path}',tofile=f'b/{path}'))
    append(runtime,base,active)
    append(tests,active,combined)
for target,lines in [('mac-full-refresh.patch',runtime),('mac-full-refresh-tests.patch',tests)]:
    (here/target).write_text(''.join(lines))
