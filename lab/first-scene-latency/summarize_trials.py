"""Summarize only complete game runs. Use the host's monotonic clock for all timing."""
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
NAMES = ("schedule-ornp07qk", "xmem-abba-usxk0alo", "xmem-baab-kl4wv5zg",
         "snapshot-abba-ge6w564l")


def summary(run):
    folder = HERE / "runs" / run
    meta = json.loads((folder / "run.json").read_text())
    if meta.get("exit") != 0:
        raise RuntimeError(f"{run} is incomplete")
    details = folder / "render.json"
    subprocess.run([sys.executable, HERE / "trace/render_latency.py", folder, "-o", details], check=True)
    data = json.loads(details.read_text())
    compact = {}
    for arm, stats in data["by_mode"].items():
        compact[arm] = {
            "frames": stats["n"],
            "frame_p50_ms": stats["wall_ms"]["p50"],
            "frame_p95_ms": stats["wall_ms"]["p95"],
            "frame_mean_ms": stats["wall_ms"]["mean"],
            "draws_p50": stats["draws"]["p50"],
            "exclusive_mean_ms": {k: v["mean"] for k, v in stats["exclusive_ms"].items()},
            "nested_mean_ms": {k: v["mean"] for k, v in stats["nested_ms"].items()},
            "nested_p50_ms": {k: v["p50"] for k, v in stats["nested_ms"].items()},
        }
    return {
        "sha256": meta["sha"], "relative_start": data["min_start_offset"],
        "world_frame": data["world_frame"], "missing_intervals": data["skipped_anchor_intervals"],
        "unparsed": data["unparsed_render_messages"],
        "details": str(details.relative_to(HERE)), "arms": compact,
        "hot_resource": data["hot_calls_by_mode"],
        "actual_snapshot_groups": {
            arm: dict(frames=stats["n"], snapshot_ms=stats["nested_ms"]["snapshot_with_declarations"],
                      wall_ms=stats["wall_ms"], declarations=stats["snapshot_declarations"])
            for arm, stats in data["by_applied_snapshot_mode"].items()
        } if data["experiment"] == "snapshots" else {},
    }


if __name__ == "__main__":
    report = {name: summary(name) for name in NAMES}
    (HERE / "trials.json").write_text(json.dumps(report,indent=2) + "\n")
