from pathlib import Path
from statistics import median
import json,re,subprocess
here=Path(__file__).resolve().parent
wrap=(here/"wrap.log").read_text()
reps={mode:{} for mode in ("stage","import")}
for round_id,mode,elapsed,equal in re.findall(r'round=(\d+) mode=(\w+) elapsed_ms=([.\d]+) equal=(\d+)',wrap):
    if equal!='1':
        raise ValueError("readback mismatch")
    reps[mode][int(round_id)]=float(elapsed)
for vals in reps.values():
    assert sorted(vals)==list(range(10))
xenia=(here/"xenia.log").read_text()
for kind in ('copy','alias'):
    rows=re.findall(r"mode="+kind+r" round=(\d+) known_span_apply_us=([.\d]+) equal=(\d+)",xenia)
    assert len(rows)==8 and all(x[2]=='1' for x in rows)
assert 'mode=alias changed_after_selection=1 expected=1' in xenia
assert 'mode=copy changed_after_selection=0 expected=0' in xenia
assert 'passed: 0 wrong results' in (here/"upstream-heap.log").read_text()
assert (here/"capability.log").read_text().count("heap=present")==3
data=dict(hardware="Apple M5 Max",mullion_tested_release="v0.5-alpha.4",
          mullion_release_commit=subprocess.check_output(["git","rev-parse","HEAD"],cwd=here/"mullion-release",text=True).strip(),
          capture_layout=(here/"layout.txt").read_text().splitlines(),
          byte_source="deterministic synthetic bytes, data held stable through fence",
          mullion=dict(rounds=10, matched_readbacks=20, resource_bytes=174*1024*1024,
                      steady_wall_ms_median={mode:median(v[i] for i in range(1,10)) for mode,v in reps.items()},
                      paired_wall_ms=reps, one_time_import_ms=float(re.search(r'import_ms=([.\d]+)',wrap)[1])),
          xenia=dict(test_scope="unmodified Initialize and UploadRanges in a host harness with real Metal",
                     resource_bytes=174*1024*1024,synthetic_page_lists=8,equal_readbacks_per_mode=8,
                     delayed_cpu_change_bytes=1,old_version_wrong_bytes={"direct_alias":1,"copy":0}))
(here/"results.json").write_text(json.dumps(data,indent=2)+"\n")
print(data['mullion']['steady_wall_ms_median'])
