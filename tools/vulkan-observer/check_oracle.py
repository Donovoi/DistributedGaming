"""Compare a private native-oracle case with and without the observer. Linux only."""
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys


def forbid_file_writes():
    # Pipes remain writable. The child can return its oracle JSON on stdout.
    signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
    resource.setrlimit(resource.RLIMIT_FSIZE, (0, 0))


def main():
    if len(sys.argv) != 6:
        raise SystemExit("usage: check_oracle.py ORACLE SHADER HISTOGRAM UNIFORM NEW_RUN_DIR")
    command = [str(Path(p).resolve(strict=True)) for p in sys.argv[1:5]]
    run = Path(sys.argv[5]).resolve()
    run.mkdir(parents=True, exist_ok=False)
    layer = Path(__file__).resolve().parent
    results = {}
    for mode in ("baseline", "observer", "missing-log", "existing-log", "write-failure"):
        env = os.environ.copy()
        for key in ("VK_INSTANCE_LAYERS", "VK_ADD_LAYER_PATH", "DG_OBSERVER_LOG"):
            env.pop(key, None)
        log = run / (mode + ".jsonl")
        if mode == "missing-log":
            log = run / "absent-directory" / "events.jsonl"
        if mode == "existing-log":
            log.write_text("KEEP THIS FILE\n", encoding="utf-8")
        if mode != "baseline":
            env.update(VK_ADD_LAYER_PATH=str(layer), VK_INSTANCE_LAYERS="VK_LAYER_DG_observer",
                       DG_OBSERVER_LOG=str(log))
        result = subprocess.run(command, env=env, capture_output=True, timeout=45,
                                preexec_fn=forbid_file_writes if mode == "write-failure" else None)
        (run / (mode + ".stderr")).write_bytes(result.stderr)
        (run / (mode + ".json")).write_bytes(result.stdout)
        assert result.returncode == 0, (mode, result.returncode)
        data = json.loads(result.stdout)
        assert data["original_shader_executed"] and data["synchronization_validation"]
        assert data["validation_errors"] == 0
        assert len(data["runs"]) == 3
        assert all(r["inputs_unchanged"] and r["guards_unchanged"] for r in data["runs"])
        results[mode] = data
        if mode == "existing-log":
            assert log.read_text(encoding="utf-8") == "KEEP THIS FILE\n"
        if mode == "write-failure":
            assert log.exists() and log.stat().st_size == 0
    assert all(data == results["baseline"] for data in results.values())
    events = [json.loads(line) for line in (run / "observer.jsonl").read_text().splitlines()]
    assert events[0]["event"] == "observer_start"
    dispatches = [e for e in events if e["event"] == "target_dispatch_recorded"]
    submissions = [e for e in events if e["event"] == "target_submission"]
    assert len(dispatches) == len(submissions) == 3
    assert all(e["groups"] == [1, 1, 1] and not e["command_unknown"] for e in dispatches)
    assert all(e["success"] and e["command_list_complete"] and not e["unknown_commands"] for e in submissions)
    tail = events[-1]
    assert tail["event"] == "instance_lifetime_end" and tail["open_instances"] == 0
    assert tail["recorded_target_dispatches"] == tail["submitted_target_dispatches"] == 3
    assert tail["dropped_records"] == tail["unsupported_observations"] == 0
    assert not any(tail[k] for k in ("log_io_failed", "observation_panicked", "observation_disabled",
                                    "execution_proven", "resource_contract_complete"))
    print(json.dumps({"modes_passed": list(results), "dispatches_per_mode": 3,
                      "output_bits": [r["output_bits"] for r in results["baseline"]["runs"]],
                      "validation_errors": 0, "observer_tail": tail}, indent=2))


if __name__ == "__main__":
    main()
