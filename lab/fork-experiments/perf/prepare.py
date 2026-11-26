from pathlib import Path
import json, subprocess
here=Path(__file__).resolve().parent
root=here.parents[1]
src=here/'src'
gpu=Path('crates/victory-gpu/src')
def original(name):
    return subprocess.check_output(['git','show',f'HEAD:{gpu/name}'],cwd=src,text=True)
path=src/gpu/'indirect.rs'
s=original(path.name)
a=s.index('let adapter: IDXGIAdapter = factory.EnumWarpAdapter()')
b=s.index('let mut device: Option<ID3D12Device>',a)
s=s[:a]+'''let adapter = factory.EnumAdapters1(0).expect("no hardware adapter");
                let desc = adapter.GetDesc1().unwrap();
                assert_eq!(desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32, 0, "expected a hardware GPU");
                println!("hardware {}", String::from_utf16_lossy(&desc.Description).trim_end_matches('\\0'));
                '''+s[b:]
a=s.index('\n}',s.index('assert_eq!(hits, 5,'))
path.write_text(s[:a]+'\n'+(here/'captured.inc.rs').read_text()+s[a:])
path=src/gpu/'host.rs'
path.write_text(original(path.name)+'''
#[cfg(test)]
impl Host {
    pub(crate) fn experiment_time_frequency(&self) -> u64 {
        unsafe { self.queue.GetTimestampFrequency().unwrap_or(0) }
    }
}
''')
paths=sorted((root/'victory-frames').glob('picture-20261125-063600-txquo33l/dispatch-*/arguments.bin'))+sorted((root/'victory-frames').glob('picture-20261125-065957-caea263n/dispatch-*/arguments.bin'))
assert len(paths)==5 and all(p.stat().st_size==12 for p in paths)
(src/gpu/'captured-inputs.bin').write_bytes(b''.join(p.read_bytes() for p in paths))
(here/'capture-paths.json').write_text(json.dumps([str(p.relative_to(root)) for p in paths],indent=2)+'\n')

path=src/gpu/'list.rs'
s=original(path.name)
offset=s.index('    #[test]',s.index('mod tests {'))
path.write_text(s[:offset]+(here/'groups.inc.rs').read_text()+'\n'+s[offset:])
