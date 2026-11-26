"""Measure first-scene present intervals and cross-check their frame numbers."""
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from statistics import median
import json
import math
import re
import sys


WORLD = re.compile(r"world's first frame .*?frame=(\d+)")
CAPTURE = re.compile(r"frame saved path=.*[\\/]frame-(\d+)\.png$")
BIND = re.compile(r"first bind .*?frame=(\d+)")
METER = re.compile(r"fps, ([\d.]+) ms/frame \(")
CHECKPOINTS = (8, 16, 24, 30, 40, 48)


class InvalidLog(ValueError):
    pass


@dataclass
class Present:
    time: datetime
    line: int
    anchored: bool
    meter_ms: float | None = None
    capture_delay_ms: float | None = None


def stamp(line):
    return datetime.fromisoformat(line.split(maxsplit=1)[0])


def measures(path):
    first = None
    started = False
    pending_capture = None
    tokens = set()
    rows = []
    captures = set()
    for line_no, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
        if match := WORLD.search(line):
            if first is not None:
                raise InvalidLog("multiple world starts")
            first = int(match[1])
        if first is None:
            continue
        if match := CAPTURE.search(line):
            absolute = int(match[1])
            if absolute == first + 8:
                if started:
                    raise InvalidLog("duplicate +8 capture")
                started = True
            if started and absolute <= first + CHECKPOINTS[-1]:
                if pending_capture:
                    raise InvalidLog(f"line {line_no}: two captures without a present")
                pending_capture = absolute, stamp(line)
        if not started:
            continue
        if match := BIND.search(line):
            tokens.add(int(match[1]))
        if "title memory this frame:" in line:
            offset = len(rows) + 8
            if offset > CHECKPOINTS[-1]:
                break
            expected = first + offset
            now = stamp(line)
            anchored = bool(tokens)
            if tokens and tokens != {expected}:
                raise InvalidLog(f"line {line_no}: expected frame {expected}, shader tokens {sorted(tokens)}")
            tokens.clear()
            row = Present(now, line_no, anchored)
            if pending_capture:
                absolute, time = pending_capture
                delay = (now - time).total_seconds()
                if absolute != expected or not 0 <= delay <= .100:
                    raise InvalidLog(f"line {line_no}: capture {absolute}, expected {expected}, lag {delay:.4f}s")
                row.anchored = True
                row.capture_delay_ms = round(delay * 1000, 3)
                captures.add(offset)
                pending_capture = None
            if rows and now <= rows[-1].time:
                raise InvalidLog(f"line {line_no}: non-increasing present clock")
            rows.append(row)
        if match := METER.search(line):
            if not rows:
                raise InvalidLog(f"line {line_no}: performance meter without present")
            row = rows[-1]
            delay = (stamp(line) - row.time).total_seconds()
            if not 0 <= delay <= .100 or row.meter_ms is not None:
                raise InvalidLog(f"line {line_no}: meter does not belong to a unique present")
            row.meter_ms = float(match[1])

    if not {8, 16, 24} <= captures:
        raise InvalidLog(f"missing required +8/+16/+24 capture confirmation, found {sorted(captures)}")
    if len(rows) < 23:
        raise InvalidLog(f"ends at +{len(rows) + 7} before +30")
    # A shader token or screenshot fixes an absolute endpoint. Counts between
    # two such anchors must agree. Beyond the final anchor every interval must
    # independently match the renderer's separately measured per-frame time.
    last_anchor = max(i for i, row in enumerate(rows) if row.anchored)
    for i, row in enumerate(rows[1:], 1):
        delta_ms = (row.time - rows[i - 1].time).total_seconds() * 1000
        if row.meter_ms is not None and abs(delta_ms - row.meter_ms) > 5:
            raise InvalidLog(f"line {row.line}: gap {delta_ms:.2f} ms, renderer {row.meter_ms:.2f} ms")
        if i > last_anchor and row.meter_ms is None:
            raise InvalidLog(f"line {row.line}: interval past last anchor lacks independent confirmation")

    def delta(a, b):
        return (rows[b - 8].time - rows[a - 8].time).total_seconds()

    def window(a, b):
        intervals = [delta(i - 1, i) for i in range(a + 1, b + 1)]
        peak = max(intervals)
        end = a + 1 + intervals.index(peak)
        return {
            "frames": len(intervals),
            "total_s": delta(a, b),
            "mean_s": round(delta(a, b) / len(intervals), 6),
            "median_s": round(median(intervals), 6),
            "p95_nearest_rank_s": sorted(intervals)[math.ceil(.95 * len(intervals)) - 1],
            "max_s": peak,
            "max_from_to": [end - 1, end],
        }

    return {
        "log": path.name,
        "world_frame": first,
        "last_verified_offset": len(rows) + 7,
        "capture_to_present_ms": {f"+{i}": rows[i - 8].capture_delay_ms for i in sorted(captures)},
        "present_utc": {f"+{i}": rows[i - 8].time.isoformat() if i - 8 < len(rows) else None for i in CHECKPOINTS},
        "windows": {f"+{a}..+{b}": window(a, b) for a, b in [(8, 24), (24, 30), (8, 30), (30, 40), (8, 40), (40, 48), (8, 48)] if b - 8 < len(rows)},
        "successive_intervals_s_from_8_to_30": [delta(i - 1, i) for i in range(9, 31)],
    }


if __name__ == "__main__":
    if not sys.argv[1:]:
        sys.exit("usage: python3 frame-times.py LOG [LOG ...]")
    try:
        print(json.dumps([measures(Path(arg)) for arg in sys.argv[1:]], indent=2))
    except (InvalidLog, OSError) as error:
        sys.exit(f"Rejected log: {error}")
