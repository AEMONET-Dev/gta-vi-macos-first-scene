from pathlib import Path
import subprocess
here=Path(__file__).resolve().parent
src=here/'compat-src'
mac=here.parent/'repos/VIctory-macOS'
gpu=Path('crates/victory-gpu/src')
def save(name, transform):
    content=subprocess.check_output(['git','show',f'HEAD:{name}'],cwd=src,text=True)
    (src/name).write_text(transform(content))
def replace(s,a,b):
    if s.count(a)!=1: raise ValueError((a,s.count(a)))
    return s.replace(a,b)
mac_watch=(mac/gpu/'watch.rs').read_text()
mode=mac_watch[mac_watch.index('pub fn fault_tracking()'):mac_watch.index('/// The unit tracked and copied')]
def watch(s):
    s=replace(s,'/// The unit tracked and copied:',mode+'/// The unit tracked and copied:')
    s=replace(s,'use std::sync::{Mutex, RwLock};','use std::sync::{LazyLock, Mutex, RwLock};')
    for name,ret in [('ask_ahead',''),('ask_once',''),('asked','None'),('writes_guarded','None'),('writes_kind','None'),('view_guard','false')]:
        at=s.index('{',s.index('fn '+name+'('))+1
        s=s[:at]+'\n    if !fault_tracking() { return'+((' '+ret) if ret else '')+'; }'+s[at:]
    return s
save(gpu/'watch.rs',watch)
save(gpu/'lib.rs',lambda s:replace(s,'pub use watch::{','pub use watch::{\n    fault_tracking,'))
save(Path('crates/victory-hle/src/modules/xmem.rs'),lambda s:replace(s,'if kind & MEM_RESERVE_PLACEHOLDER == 0 {\n            kind |= MEM_WRITE_WATCH;', 'if kind & MEM_RESERVE_PLACEHOLDER == 0 && victory_gpu::fault_tracking() {\n            kind |= MEM_WRITE_WATCH;'))
def draw(s):
    s=replace(s,'if FRAME.swap(frame, Ordering::Relaxed) == frame {','if crate::fault_tracking() && FRAME.swap(frame, Ordering::Relaxed) == frame {')
    return replace(s,'if shadow.unwatched() {','if crate::fault_tracking() && shadow.unwatched() {')
save(gpu/'draw.rs',draw)
# Serialize texture refreshes as usual. The full snapshot is made through the
# existing sparse-safe copy, under the same mapping read-lock used by texture.rs.
save(gpu/'texture.rs',lambda s:replace(s,'let last = &mut *last;\n        if let Some((now, written))','''let last = &mut *last;
        if !crate::fault_tracking() {
            let _mapped = crate::watch::title_memory_reading();
            with_read(size, |read| {
                crate::host::copy_mapped(read.as_mut_ptr(), self.memory, size, false);
                self.detile(read);
            });
            return size;
        }
        if let Some((now, written))'''))
if (here/'compat-test.rs').exists():
    save(gpu/'shadow.rs',lambda s:s+'\n'+(here/'compat-test.rs').read_text())
tex = src/gpu/'texture.rs'
if (here/'compat-texture-test.rs').exists():
    tex.write_text(tex.read_text()+'\n'+(here/'compat-texture-test.rs').read_text())
xmem=src/'crates/victory-hle/src/modules/xmem.rs'
xmem.write_text(replace(xmem.read_text(),'assert_eq!(watched, 0);','assert_eq!(watched == 0, victory_gpu::fault_tracking());'))
