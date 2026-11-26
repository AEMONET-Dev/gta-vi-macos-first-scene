"""Attribute delivered world-frame wall time from the host's render_latency events."""
import argparse
from collections import Counter, defaultdict
from contextlib import nullcontext
import json
import math
from pathlib import Path
import re
from statistics import mean, median
import sys

POINTS = ("req", "acquired", "descriptors", "heaps", "prepared", "gpu_end", "writeback", "done")
STAGES = ("queue_wait", "descriptors", "texture_heaps", "deferred", "submit_fence",
          "after_fence_checks", "writeback")
BUSY = set(STAGES) - {"queue_wait"}
EXCLUSIVE = ("descriptors", "texture_heaps", "deferred_buffer_batch", "deferred_other",
             "submit_fence", "after_fence_checks", "writeback", "multiple_submission_stages",
             "queue_wait_only", "outside_recorded_submission")
FENCE_POINTS = ("started", "submitted", "signaled", "armed", "returned")
FENCE_STAGES = ("fence_execute_call", "fence_signal_call", "fence_arm_event_call", "fence_wait_return")
DETAIL = FENCE_STAGES + ("fence_without_wait_return", "snapshot_union", "snapshot_with_declarations")
NESTED = ("submission_active", "queue_wait_union", "queue_wait_while_active",
          "buffer_batch_union", "buffer_outside_deferred", "hot_copy_union") + DETAIL
KV = re.compile(r'\b([a-zA-Z_][a-zA-Z_0-9]*)=(0x[0-9a-fA-F]+|[0-9]+|true|false)\b')
PNG = re.compile(r'frame saved path=.*?[\\/]frame-(\d+)\.png\b')
ANSI = re.compile(r'\x1b\[[0-9;]*m')


def planned(policy, offset):
    if policy in ("off", "on"):
        return policy
    if policy not in ("abba", "baab"):
        return None
    if offset < 8:
        return "off"
    middle = (offset - 8) // 2 % 4 in (1, 2)
    return "on" if middle == (policy == "abba") else "off"


def read_events(lines):
    world, saved, anchors, spans, batches, hot = None, set(), [], [], [], []
    fences, snapshots = [], []
    png_since_anchor = Counter()
    capture_activity = {}
    unparsed = Counter()
    for n, raw in enumerate(lines, 1):
        if not any(text in raw for text in ("render_latency", "world's first frame", "frame saved path")):
            continue
        line = ANSI.sub("", raw)
        fields = KV.findall(line)
        vals = {k: (v == "true" if v in ("true", "false") else int(v, 0)) for k, v in fields}
        try:
            if "world's first frame" in line:
                if world is not None and world != vals["frame"]:
                    raise ValueError("multiple world markers from different runs")
                world = vals["frame"]
            elif "frame saved path" in line:
                if m := PNG.search(line):
                    saved.add(int(m[1]))
                    png_since_anchor["frame"] += 1
                elif "material-" in line:
                    png_since_anchor["material"] += 1
                else:
                    png_since_anchor["other"] += 1
            elif "render_latency" in line:
                if "frame delivered" in line:
                    anchors.append((vals["t"], vals["frame"], vals["drawn"], vals["dispatched"]))
                    capture_activity[vals["t"]] = dict(png_since_anchor)
                    png_since_anchor.clear()
                elif "submission timeline" in line:
                    times = tuple(vals[k] for k in POINTS)
                    if any(a > b for a, b in zip(times, times[1:])):
                        raise ValueError("submission stamps out of order")
                    spans.extend((a, b, k) for k, a, b in zip(STAGES, times, times[1:]))
                elif "fence timeline" in line:
                    times = tuple(vals[k] for k in FENCE_POINTS)
                    if any(a > b for a, b in zip(times, times[1:])):
                        raise ValueError("fence stamps out of order")
                    stages = FENCE_STAGES if vals["waited"] else FENCE_STAGES[:3] + ("fence_without_wait_return",)
                    spans.extend((a, b, k) for k, a, b in zip(stages, times, times[1:]))
                    fences.append((times[0], times[-1], vals))
                elif "snapshot timeline" in line:
                    begin, end = vals["begin"], vals["end"]
                    if begin > end or not isinstance(vals["merge"], bool):
                        raise ValueError("invalid snapshot")
                    spans.append((begin, end, "snapshot_union"))
                    if vals["declarations"] > 0:
                        spans.append((begin, end, "snapshot_with_declarations"))
                    snapshots.append((begin, end, vals))
                elif "buffer batch" in line:
                    begin, end = vals["begin"], vals["end"]
                    flags = [k for k in ("xmem", "schedule") if k in vals]
                    if begin > end or len(flags) != 1 or not isinstance(vals[flags[0]], bool):
                        raise ValueError("invalid batch")
                    spans.append((begin, end, "batch"))
                    batches.append((begin, end, vals))
                elif "174MiB refresh" in line:
                    begin, end = vals["begin"], vals["end"]
                    if begin > end:
                        raise ValueError("unordered hot-copy stamp")
                    spans.append((begin, end, "hot"))
                    hot.append((begin, end, vals))
                else:
                    unparsed[re.sub(r'\b\w+=\S+', '', line.split('render_latency:', 1)[-1]).strip()[:120]] += 1
        except (ValueError, KeyError) as exc:
            raise ValueError(f"line {n}: {exc}") from exc
    if world is None or len(anchors) < 2:
        raise ValueError("need a world marker and two render_latency frame delivered events")
    # Events may be written by other threads after a present. Use host monotonic stamps.
    return world, saved, sorted(anchors), spans, batches, hot, fences, snapshots, unparsed, capture_activity


def partition(start, end, spans):
    events = defaultdict(Counter)
    events[start]
    events[end]
    for left, right, stage in spans:
        a, b = max(left, start), min(right, end)
        if a < b:
            events[a][stage] += 1
            events[b][stage] -= 1
    active = Counter()
    exclusive, nested = dict.fromkeys(EXCLUSIVE, 0), dict.fromkeys(NESTED, 0)
    previous = start
    for point, changes in sorted(events.items()):
        duration = point - previous
        stages = {k for k in BUSY if active[k] > 0}
        waiting, batch = active["queue_wait"] > 0, active["batch"] > 0
        if len(stages) > 1:
            key = "multiple_submission_stages"
        elif stages == {"deferred"}:
            key = "deferred_buffer_batch" if batch else "deferred_other"
        elif stages:
            key = next(iter(stages))
        else:
            key = "queue_wait_only" if waiting else "outside_recorded_submission"
        exclusive[key] += duration
        for label, present in (("submission_active", bool(stages)), ("queue_wait_union", waiting),
                               ("queue_wait_while_active", waiting and bool(stages)),
                               ("buffer_batch_union", batch), ("buffer_outside_deferred", batch and "deferred" not in stages),
                               ("hot_copy_union", active["hot"] > 0)):
            if present:
                nested[label] += duration
        for label in DETAIL:
            if active[label] > 0:
                nested[label] += duration
        active.update(changes)
        previous = point
    assert sum(exclusive.values()) == end - start
    return exclusive, nested


def ms_dict(times):
    return {k: v / 1000 for k, v in times.items()}


def distribution(series):
    if not series:
        return None
    s = sorted(series)
    return dict(n=len(s), mean=round(mean(s), 3), p50=round(median(s), 3),
                p95=round(s[math.ceil(.95*len(s))-1], 3))


def summarize(frames):
    return {
        "n": len(frames),
        "wall_ms": distribution([r["wall_ms"] for r in frames]),
        "draws": distribution([r["draws"] for r in frames]),
        "dispatches": distribution([r["dispatches"] for r in frames]),
        "exclusive_ms": {k: distribution([r["exclusive_ms"][k] for r in frames]) for k in EXCLUSIVE},
        "nested_ms": {k: distribution([r["nested_ms"][k] for r in frames]) for k in NESTED},
        "snapshot_declarations": distribution([r["snapshot_declarations_started"] for r in frames]),
        "snapshot_callbacks": distribution([r["snapshot_calls_started"] for r in frames]),
        "fence_callbacks": distribution([r["fence_calls_started"] for r in frames]),
        "batch_calls_started": distribution([r["batch_calls_started"] for r in frames]),
        "hot_calls_started": distribution([r["hot_calls_started"] for r in frames]),
        "hot_worker_ms": distribution([r["hot_worker_ms"] for r in frames]),
        "hot_attempts_started": sum(r["hot_attempts_started"] for r in frames),
        "hot_successes_started": sum(r["hot_successes_started"] for r in frames),
        "hot_stored_bytes_started": sum(r["hot_stored_bytes_started"] for r in frames),
        "hot_requested_bytes_started": sum(r["hot_requested_bytes_started"] for r in frames),
    }


def touches(left, right, start, end):
    return max(left, start) < min(right, end) or left == right and start <= left < end


def mode_label(observed):
    if len(observed) == 1:
        return next(iter(observed))
    return "mixed" if observed else "unknown"


def analyze(lines, policy=None, min_offset=8, subject=None):
    world, captures, anchors, spans, batches, hot, fences, snapshots, unparsed, capture_activity = read_events(lines)
    labels = {k for _, _, v in batches for k in ("schedule", "xmem") if k in v}
    if len(labels) > 1:
        raise ValueError("two experiment types in one log")
    batch_flag = next(iter(labels), None)
    # A v2 xmem run also emits snapshot rows; metadata identifies which feature is varied.
    if subject == "snapshots":
        experiment = subject
    else:
        experiment = batch_flag or ("xmem" if hot else subject or "unknown")
    rows, skipped = [], []
    for a, b in zip(anchors, anchors[1:]):
        start, prev, prev_draws, prev_dispatches = a
        end, frame, draws, dispatches = b
        if prev < world:
            continue
        if frame != prev + 1 or end <= start or draws < prev_draws or dispatches < prev_dispatches:
            skipped.append({"previous_frame": prev, "frame": frame, "start_us": start, "end_us": end})
            continue
        exclusive, nested = partition(start, end, spans)
        # Half-open intervals: counts/bytes belong to a call's start frame. Time always intersects.
        live = [v for l, r, v in batches if touches(l, r, start, end)]
        begun = [v for l, _, v in batches if start <= l < end]
        schedules = Counter("on" if v[batch_flag] else "off" for v in live)
        # A batch samples xmem at its end. The actual per-copy branch reads enabled at its start.
        hot_modes = Counter("on" if v["enabled"] else "off" for l, r, v in hot if touches(l, r, start, end))
        snap_live = [v for l, r, v in snapshots if touches(l, r, start, end)]
        snap_started = [v for l, _, v in snapshots if start <= l < end]
        snap_modes = Counter("on" if v["merge"] else "off" for v in snap_live)
        active_snap_modes = Counter("on" if v["merge"] else "off" for v in snap_live if v["declarations"])
        observed = set(snap_modes) if experiment == "snapshots" else set(schedules) | set(hot_modes)
        mode = mode_label(observed)
        fences_started = [v for l, _, v in fences if start <= l < end]
        h_start = [v for l, _, v in hot if start <= l < end]
        h_dur = sum(max(0, min(r, end) - max(l, start)) for l, r, _ in hot)
        offset = prev - world
        obs = {
            "frame": frame, "offset": frame-world, "start_offset": offset, "start_us": start, "end_us": end,
            "wall_ms": (end-start)/1000, "draws": draws-prev_draws, "dispatches": dispatches-prev_dispatches,
            "end_frame_captured": frame in captures, "png_saves_in_interval": capture_activity.get(end, {}),
            "mode": mode, "planned_start": planned(policy, offset),
            "planned_end": planned(policy, offset + 1), "cycle": (offset-8)//8 if offset >= 8 else None,
            "batch_modes_intersected": dict(schedules), "hot_modes_intersected": dict(hot_modes), "batch_calls_started": len(begun),
            "planned_bytes_started": sum(v["planned"] for v in begun), "copied_bytes_started": sum(v["copied"] for v in begun),
            "fence_calls_started": len(fences_started),
            "fence_calls_without_wait_started": sum(not v["waited"] for v in fences_started),
            "snapshot_calls_started": len(snap_started),
            "snapshot_declarations_started": sum(v["declarations"] for v in snap_started),
            "snapshot_modes_intersected": dict(snap_modes),
            "snapshot_applied_mode": mode_label(active_snap_modes) if active_snap_modes else "no_declarations",
            "hot_attempts_started": sum(v["attempted"] for v in h_start),
            "hot_successes_started": sum(v["success"] for v in h_start),
            "hot_stored_bytes_started": sum(v["stored"] for v in h_start),
            "hot_requested_bytes_started": sum(v["requested"] for v in h_start),
            "hot_calls_started": len(h_start), "hot_worker_ms": h_dur/1000,
            "exclusive_ms": ms_dict(exclusive), "nested_ms": ms_dict(nested),
        }
        rows.append(obs)
    if not rows:
        raise ValueError("no consecutive world presents with monotonic timestamps and work counters")
    selected = [r for r in rows if r["start_offset"] >= min_offset]
    grouped = defaultdict(list)
    for frame in selected:
        grouped[frame["mode"]].append(frame)
    hot_summary = {}
    selected_stamps = [(row["start_us"], row["end_us"]) for row in selected]
    for label, enabled in (("off", False), ("on", True)):
        calls = [v for l, _, v in hot if v["enabled"] == enabled and any(a <= l < b for a, b in selected_stamps)]
        hot_summary[label] = {"n": len(calls), "successes": sum(v["success"] for v in calls),
                    "attempts": sum(v["attempted"] for v in calls),
                    "elapsed_us": distribution([v["elapsed_us"] for v in calls]),
                    "attempted_us": distribution([v["attempted_us"] for v in calls if "attempted_us" in v]),
                    "stored_bytes": sum(v["stored"] for v in calls), "requested_bytes": sum(v["requested"] for v in calls)}
    return {
        "experiment": experiment, "subject_from_settings_or_cli": subject, "hot_calls_by_mode": hot_summary,
        "v2_event_counts": {"fences": len(fences), "snapshots": len(snapshots)},
        "by_applied_snapshot_mode": {
            k: summarize([r for r in selected if r["snapshot_applied_mode"] == k])
            for k in sorted({r["snapshot_applied_mode"] for r in selected})
        } if experiment == "snapshots" else {},
        "world_frame": world, "policy": policy, "min_start_offset": min_offset,
        "frame_anchor_count": len(anchors), "skipped_anchor_intervals": skipped,
        "unparsed_render_messages": dict(unparsed), "hot_events": [v for _, _, v in hot],
        "all_frames": summarize(rows), "selected_frames": summarize(selected),
        "by_mode": {k: summarize(v) for k, v in grouped.items()},
        "by_mode_without_png_saves": {k: summarize([r for r in v if not r["png_saves_in_interval"]])
                                      for k, v in grouped.items()},
        "by_mode_capture": {k: {label: summarize([r for r in v if r["end_frame_captured"] == state])
                                for label, state in (("captured", True), ("uncaptured", False))}
                            for k, v in grouped.items()},
        "frames": rows,
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("input", type=Path, help="game log, completed run.json, or run directory")
    p.add_argument("--mode", choices=["on", "off", "abba", "baab"], help="policy from run.json, or an explicit override")
    p.add_argument("--subject", choices=["schedule", "xmem", "snapshots"], help="experiment varied; read from run.json if available")
    p.add_argument("--min-offset", type=int, default=8, help="include frames whose opening world offset is at least this value in mode summaries")
    p.add_argument("-o", "--output", type=Path, help="JSON output (default stdout)")
    args = p.parse_args()
    path, mode, subject = args.input, args.mode, args.subject
    if path.is_dir():
        path = path / "run.json"
    if path.name == "run.json":
        meta = json.loads(path.read_text())
        mode = mode or meta["mode"]
        subject = subject or meta.get("settings", {}).get("VICTORY_LATENCY_SUBJECT")
        if not meta.get("log"):
            p.error("run.json has no log field yet; pass the game log directly")
        path = Path(meta["log"])
    try:
        with path.open(errors="replace") as stream:
            data = analyze(stream, mode, args.min_offset, subject)
    except (ValueError, OSError) as exc:
        p.error(str(exc))
    data["log"] = str(path.resolve())
    with (args.output.open("w") if args.output else nullcontext(sys.stdout)) as out:
        json.dump(data, out, indent=2)
        out.write("\n")


if __name__ == "__main__":
    main()
