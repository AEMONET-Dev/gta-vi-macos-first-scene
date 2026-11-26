"""Per delivered frame: how long buffer-refresh batches held up the submission,
split by what the batch's critical worker (the one it waited for) did.

Input: a game log from a build with `VICTORY_REFRESH_TIMING=1`
(`claude/performance`, host/refresh_timing.rs). Each batch line is stamped when
the batch ended; the batch occupied the submitting thread for `wall_us` before
that. Batches run synchronously inside the submission's preparation, so their
wall time is time the next delivered frame waited.

Frames are the `title memory this frame:` present markers after the world's
first frame (the same markers tmp/submit-perf-9njeneg8/frame-times.py
verifies against captures). A batch is charged to the interval its end falls in.

Columns per frame, in ms:
  wall           sum of batch wall times
  first_ok       critical worker: whole-span reads that succeeded
  first_failed   critical worker: whole-span reads that failed on a hole
                 (Wine copies up to the hole, then reports failure: wasted)
  query          critical worker: VirtualQuery mapping lookups
  retry          critical worker: per-region re-reads and salvage reads
  zero           critical worker: zero-filling unreadable regions
  other          critical worker's job time outside those probes
  idle           batch wall time the critical worker was not running a job
                 (scheduling, joins, planning on the submitter)
wall = first_ok + first_failed + query + retry + zero + other + idle.
"""
from datetime import datetime
from pathlib import Path
from statistics import median
import json
import math
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
WORLD = re.compile(r"world's first frame .*?frame=(\d+)")
BATCH = re.compile(r"refresh batch timing wall_us=(\d+) workers=(\d+) planned_workers=(\d+) jobs=(\d+) bytes=(\d+) "
                   r"critical_busy_us=(\d+) critical_timed_us=(\d+) \| critical: (.*?) \| all: (.*)$")
PART = re.compile(r"(\w+)=(\d+)us/(\d+)x/(\d+)MB")
KINDS = ["first_ok", "first_failed", "query", "retry", "zero"]


def stamp(line):
    return datetime.fromisoformat(line.split(maxsplit=1)[0])


def parts(text):
    return {m[1]: (int(m[2]), int(m[3]), int(m[4])) for m in PART.finditer(text)}


def analyze(path, min_offset=8):
    world, presents, batches = None, [], []
    for line in Path(path).read_text(errors="replace").splitlines():
        line = ANSI.sub("", line)
        if not line[:4].isdigit():
            continue
        if world is None and (m := WORLD.search(line)):
            world = int(m[1])
            continue
        if world is not None and "title memory this frame:" in line:
            presents.append(stamp(line))
        elif m := BATCH.search(line):
            crit = parts(m[8])
            batches.append(dict(end=stamp(line), wall=int(m[1]), workers=int(m[2]), jobs=int(m[4]), bytes=int(m[5]),
                                busy=int(m[6]), timed=int(m[7]), crit=crit, all=parts(m[9])))
    if world is None:
        raise SystemExit("no world start in log")
    frames = []
    for i in range(1, len(presents)):
        offset = i  # interval ending at the i-th present after the world started
        if offset < min_offset:
            continue
        t0, t1 = presents[i - 1], presents[i]
        mine = [b for b in batches if t0 < b["end"] <= t1]
        row = dict(offset=offset, interval_ms=(t1 - t0).total_seconds() * 1e3, batches=len(mine),
                   wall=sum(b["wall"] for b in mine) / 1e3)
        for k in KINDS:
            row[k] = sum(b["crit"].get(k, (0, 0, 0))[0] for b in mine) / 1e3
        row["other"] = sum(b["busy"] - b["timed"] for b in mine) / 1e3
        row["idle"] = sum(b["wall"] - b["busy"] for b in mine) / 1e3
        row["queries_all_workers"] = sum(b["all"].get("query", (0, 0, 0))[1] for b in mine)
        row["failed_first_MB_all"] = sum(b["all"].get("first_failed", (0, 0, 0))[2] for b in mine)
        row["zero_MB_all"] = sum(b["all"].get("zero", (0, 0, 0))[2] for b in mine)
        top = max(mine, key=lambda b: b["wall"], default=None)
        row["top_batch_ms"] = top["wall"] / 1e3 if top else 0
        frames.append(row)
    cols = ["interval_ms", "wall"] + KINDS + ["other", "idle", "top_batch_ms", "queries_all_workers",
                                              "failed_first_MB_all", "zero_MB_all"]
    p95 = lambda v: sorted(v)[math.ceil(.95 * len(v)) - 1]
    summary = {c: dict(median=round(median(r[c] for r in frames), 1), p95=round(p95([r[c] for r in frames]), 1))
               for c in cols} if frames else {}
    return dict(log=str(path), world=world, frames=len(frames), summary=summary,
                rows=[{k: (round(v, 2) if isinstance(v, float) else v) for k, v in r.items()} for r in frames])


if __name__ == "__main__":
    out = analyze(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 8)
    print(json.dumps(out, indent=1))
