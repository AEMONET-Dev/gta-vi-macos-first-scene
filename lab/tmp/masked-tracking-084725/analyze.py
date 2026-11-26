"""Summarize first-scene runs: verified present intervals (+8..+48, the same
checker the earlier comparisons used) and per-frame stage meters in that window."""
from datetime import datetime
from pathlib import Path
from statistics import median
import importlib.util
import json
import math
import re
import sys

here = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('ft', here.parents[0] / 'submit-perf-9njeneg8/frame-times.py')
ft = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ft)

METER = re.compile(r"([\d.]+) fps, ([\d.]+) ms/frame \(([\d.]+) preparing: copies ([\d.]+) ms ([\d.]+) MB, "
                   r"textures ([\d.]+) ms ([\d.]+) MB; ([\d.]+) on the GPU; ([\d.]+) copying back ([\d.]+) MB\), "
                   r"(\d+) draws/frame, dropped (\d+) without a host pipeline \((\d+) being made\) and (\d+) indirect")
COPIES = re.compile(r"copies per frame: .*?(\d+) guard faults;.*?copied whole \(not tracked\) ([\d.]+) MB in ([\d.]+) ms;"
                    r".*?guards: ([\d.]+) ms in the handler, (\d+) protect calls for (\d+) pages")
WATCHES = re.compile(r"(\d+) watches ([\d.]+) ms, ([\d.]+) MB checked, ([\d.]+) MB unchanged")


def p95(xs):
    return sorted(xs)[math.ceil(.95 * len(xs)) - 1]


def stamp(line):
    return datetime.fromisoformat(line.split(maxsplit=1)[0])


def summarize(path):
    # Some runs log with terminal colour codes; the parser expects plain text.
    clean = here / 'clean' / path.name
    clean.parent.mkdir(exist_ok=True)
    clean.write_text(re.sub(r'\x1b\[[0-9;]*m', '', path.read_text(errors='replace')))
    path = clean
    m = ft.measures(path)
    start = datetime.fromisoformat(m['present_utc']['+8'])
    last = max(v for v in m['present_utc'].values() if v)
    end = datetime.fromisoformat(last)
    intervals = []
    rows = ft.measures.__globals__  # noqa: keep parser identical
    win = m['windows'].get('+8..+48') or m['windows'].get('+8..+40') or m['windows']['+8..+30']
    # Per-interval list from the parser for +8..+30, extended to the window end.
    meters, copies, watches, info = [], [], [], {}
    audit = None
    for line in path.read_text(errors='replace').splitlines():
        if 'masked stores' in line and ('patched' in line or 'in code' in line):
            info.setdefault('masked', []).append(line.split('masked stores', 1)[1].strip()[:160])
        if 'scoped write tracking' in line:
            info.setdefault('scoped', []).append(line[line.find('scoped'):][:160] + ' ' + re.sub(r'.*?(start=\S+ len=\d+).*', r'\1', line))
        if 'track audit totals' in line:
            audit = line[line.find('same='):] if 'same=' in line else line
        if 'MISSED WRITE' in line:
            info['missed_lines'] = info.get('missed_lines', 0) + 1
        if 'EmulateForward' in line:
            info['emulate_forward'] = info.get('emulate_forward', 0) + 1
        if not line[:4].isdigit():
            continue
        try:
            t = stamp(line)
        except ValueError:
            continue
        if not (start < t <= end):
            continue
        if x := METER.search(line):
            meters.append([float(v) for v in x.groups()])
        if x := COPIES.search(line):
            copies.append([float(v) for v in x.groups()])
        if x := WATCHES.search(line):
            watches.append([float(v) for v in x.groups()])
    col = lambda rows, i: [r[i] for r in rows]
    stages = {}
    if meters:
        for name, i in [('frame_meter_ms', 1), ('preparing_ms', 2), ('buffer_refresh_ms', 3), ('buffer_refresh_MB', 4),
                        ('textures_ms', 5), ('textures_MB', 6), ('gpu_submit_wait_ms', 7), ('copy_back_ms', 8),
                        ('draws', 10), ('dropped_no_pipeline', 11), ('indirect_dropped', 13)]:
            v = col(meters, i)
            stages[name] = {'median': round(median(v), 1), 'p95': round(p95(v), 1), 'n': len(v)}
    if copies:
        for name, i in [('guard_faults', 0), ('copied_whole_MB', 1), ('copied_whole_ms', 2), ('guard_handler_ms', 3), ('protect_calls', 4)]:
            v = col(copies, i)
            stages[name] = {'median': round(median(v), 1), 'p95': round(p95(v), 1), 'n': len(v)}
    if watches:
        for name, i in [('tracked_refreshes', 0), ('tracked_take_ms', 1), ('tracked_checked_MB', 2), ('tracked_unchanged_MB', 3)]:
            v = col(watches, i)
            stages[name] = {'median': round(median(v), 1), 'p95': round(p95(v), 1), 'n': len(v)}
    return {'log': path.name, 'world_frame': m['world_frame'], 'last_verified_offset': m['last_verified_offset'],
            'window': [k for k, v in m['windows'].items() if v is win][0], 'frame_time_s': win,
            'stages_in_window': stages, 'audit_totals_last': audit, **info}


if __name__ == '__main__':
    out = []
    for arg in sys.argv[1:]:
        try:
            out.append(summarize(Path(arg)))
        except ft.InvalidLog as e:
            out.append({'log': Path(arg).name, 'rejected': str(e)})
    print(json.dumps(out, indent=1, ensure_ascii=False))
