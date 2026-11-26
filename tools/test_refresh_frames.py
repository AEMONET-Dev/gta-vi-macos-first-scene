"""Offline check of refresh_frames.py on a synthetic log with known numbers,
including a real batch line copied from the Wine unit-test output."""
import importlib.util, tempfile
from pathlib import Path
here = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rf", here / "refresh_frames.py"); rf = importlib.util.module_from_spec(spec); spec.loader.exec_module(rf)

def batch(t, wall, busy, ok, failed, q, retry, zero):
    timed = ok + failed + q + retry + zero
    crit = f"first_ok={ok}us/3x/10MB first_failed={failed}us/1x/5MB query={q}us/40x/0MB retry={retry}us/2x/1MB zero={zero}us/40x/2MB capped=0"
    return (f"2026-11-26T01:00:{t}Z  INFO victory_gpu::refresh_timing: refresh batch timing wall_us={wall} workers=4 planned_workers=4 "
            f"jobs=10 bytes=1000 critical_busy_us={busy} critical_timed_us={timed} | critical: {crit} | all: {crit}")
lines = ["2026-11-26T01:00:00.000000Z  INFO victory_gpu::capture: the world's first frame (x) frame=100"]
lines += [f"2026-11-26T01:00:{i:02d}.000000Z  INFO victory_gpu::memory: title memory this frame: 1 draws" for i in range(1, 12)]
# Two batches in the interval (10, 11]; one in (9, 10]; one before +8 (ignored).
lines += [batch("10.300000", 50000, 45000, 10000, 5000, 20000, 3000, 2000),
          batch("10.900000", 30000, 30000, 5000, 0, 15000, 5000, 1000),
          batch("09.500000", 1000, 900, 900, 0, 0, 0, 0),
          batch("03.500000", 9000, 9000, 9000, 0, 0, 0, 0)]
with tempfile.NamedTemporaryFile("w", suffix=".log", delete=False) as f:
    f.write("\n".join(sorted(lines)) + "\n")
out = rf.analyze(f.name, 8)
rows = {r["offset"]: r for r in out["rows"]}
assert set(rows) == {8, 9, 10}, rows.keys()
r = rows[10]
assert r["batches"] == 2 and r["wall"] == 80.0
assert (r["first_ok"], r["first_failed"], r["query"], r["retry"], r["zero"]) == (15.0, 5.0, 35.0, 8.0, 3.0)
assert r["other"] == 9.0 and r["idle"] == 5.0
assert abs(sum(r[k] for k in ["first_ok", "first_failed", "query", "retry", "zero", "other", "idle"]) - r["wall"]) < 1e-9
assert rows[9]["wall"] == 1.0 and rows[8]["wall"] == 0
assert rows[10]["queries_all_workers"] == 80 and rows[10]["zero_MB_all"] == 4
print("refresh_frames.py synthetic check passed:", r)
