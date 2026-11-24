"""Recompute the published M3 timing record. This does not run a game."""
import csv
import hashlib
import io
import json
import math
from pathlib import Path
import statistics


ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "docs/validation/cs2-baseline-2026-11-24"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def lf_bytes(path):
    return path.read_bytes().replace(b"\r\n", b"\n")


def close(actual, expected, label):
    require(math.isfinite(actual) and math.isfinite(expected), label + " is not finite")
    require(math.isclose(actual, expected, rel_tol=1e-10, abs_tol=1e-9), label + " differs")


def check_run(run, contents):
    require(hashlib.sha256(contents).hexdigest() == run["samples_sha256"], "Sample hash differs")
    rows = list(csv.DictReader(io.StringIO(contents.decode("utf-8"))))
    require(len(rows) == run["raw_rows"], "Row count differs")
    selected = []
    previous = -1
    for index, row in enumerate(rows):
        elapsed = int(row["elapsed_ns"])
        interval = float(row["interval_ms"])
        require(elapsed > previous and math.isfinite(interval) and interval > 0, "Invalid timing row")
        previous = elapsed
        keep = (index > 0
                and elapsed - interval * 1e6 >= run["selection_lower_elapsed_ns"]
                and elapsed <= run["selection_upper_elapsed_ns"])
        require(row["selected"] == str(int(keep)), "Selection flag differs")
        if keep:
            selected.append((elapsed, interval))
    require(len(selected) == run["kept_rows"] and len(selected) >= 100, "Selected count differs")
    duration = (selected[-1][0] - selected[0][0]) / 1e9
    require(19.3 <= duration <= 19.6, "Selected duration is outside declared bounds")
    require(0 <= run["logger_epoch_uncertainty_ms"] <= 50, "Clock uncertainty is outside bounds")
    require(19.5 <= run["resume_ack_to_pause_observed_seconds"] <= 20.5, "Playback speed differs")
    require(run["guard_ms_each_end"] == 250, "Guard differs")
    require(run["logger_stop_acknowledged"] and run["raw_csv_stable_seconds"] == 1, "Log stop was not checked")
    require(run["conditions_before"] == run["conditions_after"], "Run conditions changed")
    require(run["camera_before"] == run["camera_after"], "Camera endpoints differ")
    intervals = [v for _, v in selected]
    total = sum(intervals[1:]) / 1000
    require(abs(total-duration) <= max(0.002, duration*0.002), "Timing sum differs from timestamps")
    close(duration, run["measured_seconds"], "Duration")
    close(total, run["sum_after_first_seconds"], "Timing sum")
    close(statistics.median(intervals), run["median_present_interval_ms"], "Median")
    close(sorted(intervals)[math.ceil(0.95*len(intervals))-1], run["p95_present_interval_ms"], "p95")
    close((len(intervals)-1)/duration, run["throughput_fps"], "Throughput")


def main():
    report = json.loads((DATA / "summary.json").read_text())
    capture = json.loads((DATA / report["capture_evidence"]).read_text())
    require(report["build_id"] == capture["build_id"] and report["app_id"] == capture["app_id"], "Capture build differs")
    require([run["id"] for run in report["runs"]] == ["warmup", "01", "02", "03"], "Run set differs")
    helper = hashlib.sha256(lf_bytes(ROOT / "tools/cs2/benchmark_demo.py")).hexdigest()
    require(helper == report["collector_lf_sha256"] == report["collector_sha256"], "Collector hash differs")
    for run in report["runs"]:
        require(run["helper_sha256"] == helper, "Run collector differs")
        require(run["warmup"] == (run["id"] == "warmup"), "Warmup label differs")
        check_run(run, lf_bytes(DATA / run["samples_file"]))
        conditions = run["conditions_before"]
        require(conditions["executable_sha256"] == capture["executable_sha256"], "Executable hash differs")
        require(conditions["demo_sha256"] == capture["demo_sha256"], "Demo hash differs")
        require(conditions["video_config_sha256"] == report["video_config_sha256"], "Config hash differs")
        require(conditions["window_size"] == [1920,1080], "Window size differs")
        require(conditions["mangohud_loaded"] and not conditions["renderdoc_loaded"], "Loaded tools differ")
        require(conditions["insecure_argument"], "Insecure mode was not checked")
    require(not report["input_latency_measured"] and not report["display_latency_measured"], "Latency claim changed")
    require(not report["game_speedup_proven"], "Speedup claim changed")
    print("CS2 baseline: four sample logs, selections, identities, and statistics agree")


if __name__ == "__main__":
    main()
