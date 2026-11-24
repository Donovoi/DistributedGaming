"""Private experiment; guarded present intervals inside a fixed demo tick range."""
import argparse
import csv
import hashlib
import io
import json
import math
from pathlib import Path
import re
import shutil
import socket
import statistics
import subprocess
import time
import uuid


def now_ns():
    # MangoHud v0.6.9-1 uses MesaClock, backed by CLOCK_MONOTONIC_RAW on Linux.
    return time.clock_gettime_ns(time.CLOCK_MONOTONIC_RAW)


class Console:
    def __init__(self):
        self.s = socket.create_connection(("127.0.0.1", 21234), 2)
        self.text = ""

    def wait(self, token, start, timeout=35):
        deadline = time.monotonic() + timeout
        while token not in self.text[start:]:
            self.s.settimeout(max(0.01, deadline - time.monotonic()))
            block = self.s.recv(65536)
            if not block:
                raise RuntimeError("Console closed")
            self.text += block.decode("utf-8", "replace")
            if len(self.text) > 4 * 1024 * 1024:
                raise RuntimeError("Console reply too large")
            if time.monotonic() >= deadline:
                raise TimeoutError(token)
        return self.text[start:]

    def cmd(self, text):
        start = len(self.text)
        marker = "DG_END_" + uuid.uuid4().hex
        self.s.sendall((text + "\necho " + marker + "\n").encode())
        return self.wait(marker, start, 10)


def rows(path):
    lines = path.read_text().splitlines()
    start = next(i for i, line in enumerate(lines) if line.startswith("fps,frametime,"))
    return list(csv.DictReader(io.StringIO("\n".join(lines[start:]))))


def camera(console, expected_cap):
    text = console.cmd("getpos; fps_max; cl_demo_predict; cl_demo_freecam; spec_autodirector; demo_info")
    for required_text in ['Demo contents for dg_m3_tv.dem:', 'map_name: "de_dust2"',
                          'server_start_tick: 18156', 'patch_version: 14201', 'build_num: 10955']:
        if required_text not in text:
            raise RuntimeError('Wrong active demo identity')
    m = re.search(r"setpos ([\d.-]+) ([\d.-]+) ([\d.-]+);setang ([\d.-]+) ([\d.-]+)", text)
    if not m:
        raise RuntimeError("Camera reply missing")
    observed = [float(x) for x in m.groups()]
    if any(abs(a-b) > 0.1 for a,b in zip(observed,[182,2439,-40,0,-4])):
        raise RuntimeError("Camera moved")
    required = {"fps_max": str(expected_cap), "cl_demo_predict": "0",
                "cl_demo_freecam": "false", "spec_autodirector": "false"}
    for key, value in required.items():
        if not re.search(rf"(?m)^{key} = {value}\r?$", text):
            raise RuntimeError(f"Effective setting mismatch: {key}")
    return observed


def conditions():
    processes = []
    for proc in Path('/proc').glob('[0-9]*'):
        try:
            command = (proc/'cmdline').read_bytes().split(b'\0')
            if command and command[0].endswith(b'/bin/linuxsteamrt64/cs2'):
                processes.append((proc, command))
        except (FileNotFoundError, PermissionError):
            pass
    if len(processes) != 1:
        raise RuntimeError('Expected one CS2 process')
    proc, command = processes[0]
    if b'-insecure' not in command:
        raise RuntimeError('Game is not in insecure mode')
    maps = (proc/'maps').read_text()
    if 'libMangoHud.so' not in maps or 'librenderdoc.so' in maps:
        raise RuntimeError('Wrong measurement libraries')
    configs = list(Path('/work/steam-home/.local/share/Steam/userdata').glob('*/730/local/cfg/cs2_video.txt'))
    if len(configs) != 1:
        raise RuntimeError('Expected one video config')
    settings = hashlib.sha256(configs[0].read_bytes()).hexdigest()
    if settings != '3d1f886805396e4500c69ba0634f5f7aa0a3cc86734c83d6e5961e27c2e56ae3':
        raise RuntimeError('Video settings changed')
    env = __import__('os').environ.copy()
    env.update(DISPLAY=':97', XAUTHORITY='/work/private/display/Xauthority')
    ids = subprocess.check_output(['xdotool','search','--onlyvisible','--class','cs2'],env=env,text=True).split()
    if len(ids) != 1:
        raise RuntimeError('Expected one game window')
    geometry = subprocess.check_output(['xdotool','getwindowgeometry','--shell',ids[0]],env=env,text=True)
    fields = dict(line.split('=',1) for line in geometry.splitlines())
    size = [int(fields['WIDTH']),int(fields['HEIGHT'])]
    if size != [1920,1080]:
        raise RuntimeError('Window size changed')
    executable = hashlib.sha256((proc/'exe').read_bytes()).hexdigest()
    if executable != 'a8a299b270218ffc4cfb83eaa6c2037df5837e3bbd93f541801babf33ff5ef13':
        raise RuntimeError('Wrong game executable')
    demo_file = Path('/work/steam-home/.local/share/Steam/steamapps/common/Counter-Strike Global Offensive/game/csgo/dg_m3_tv.dem')
    demo_sha = hashlib.sha256(demo_file.read_bytes()).hexdigest()
    if demo_sha != '136bc8d43f55d2186f68acfe5a1fcfda0e81aff5cb9e55e2d3e663d0a7be6123':
        raise RuntimeError('Wrong demo file')
    return {'video_config_sha256':settings, 'window_size':size,
            'demo_sha256':demo_sha,
            'mangohud_loaded':True,'renderdoc_loaded':False,'insecure_argument':True,
            'executable_sha256':executable}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("out", type=Path)
    p.add_argument("--cap", type=int, default=0)
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    helper_sha = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    con = Console()
    control = None
    try:
        before_conditions = conditions()
        con.cmd("demo_gototick 640")
        time.sleep(0.5)
        con.cmd(f"fps_max {args.cap}; cl_demo_predict 0; cl_demo_freecam 0; spec_autodirector 0; spec_mode 6; spec_goto 182 2439 -40 0 -4; demo_timescale 1; hideconsole")
        start = len(con.text)
        con.cmd("demo_resume; demo_pauseatservertick 19436")
        con.wait("CGameRules - paused on tick 19436", start, 20)
        before_camera = camera(con, args.cap)
        logdir = Path("/dev/shm/dg-cs2-logs")
        old = set(logdir.glob("*.csv"))
        control = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        control.settimeout(3)
        control.connect("\0dg_cs2")
        control.recv(4096)
        sent = now_ns()
        control.sendall(b":logging=1;")
        deadline = time.monotonic() + 5
        first_rows = []
        while time.monotonic() < deadline:
            new = [f for f in logdir.glob("*.csv") if f not in old and not f.name.endswith("_summary.csv")]
            if len(new) == 1:
                try:
                    first_rows = rows(new[0])
                except (StopIteration, ValueError):
                    pass
                if first_rows:
                    break
            time.sleep(0.001)
        if not first_rows:
            raise RuntimeError("No fresh frame log")
        observed = now_ns()
        first_elapsed = int(first_rows[0]["elapsed"])
        epoch_lo = sent
        epoch_hi = observed - first_elapsed
        if epoch_hi < epoch_lo:
            raise RuntimeError("Logger clock bounds disagree")
        if epoch_hi - epoch_lo > 50_000_000:
            raise RuntimeError('Logger epoch uncertainty exceeds 50 ms')
        time.sleep(0.3)
        start = len(con.text)
        resume_sent = now_ns()
        con.cmd("demo_resume; demo_pauseatservertick 20716")
        con.wait("CGameRules - unpaused on tick", start, 3)
        resume_acknowledged = now_ns()
        con.wait("CGameRules - paused on tick 20716", start, 25)
        pause_received = now_ns()
        control.sendall(b":logging=0;")
        # v0.6.9-1 sends this response after closing the frame log and writing its summary.
        stop_reply = b''
        stop_deadline = time.monotonic() + 5
        # control_send omits the trailing semicolon when its parameter is empty.
        while b':LoggingFinished' not in stop_reply:
            control.settimeout(max(0.01, stop_deadline-time.monotonic()))
            block = control.recv(4096)
            if not block or len(stop_reply) + len(block) > 16384:
                raise RuntimeError('Bad logger stop response')
            stop_reply += block
            if time.monotonic() >= stop_deadline:
                raise RuntimeError('Logger stop response timed out')
        # Also require an unchanged complete file for one second before copying it.
        stable_since = time.monotonic()
        stop_deadline = stable_since + 5
        last_contents = new[0].read_bytes()
        while time.monotonic() - stable_since < 1:
            if time.monotonic() > stop_deadline:
                raise RuntimeError('Frame log did not become stable')
            time.sleep(0.1)
            contents = new[0].read_bytes()
            if contents != last_contents:
                stable_since = time.monotonic()
                last_contents = contents
        if not last_contents.endswith(b'\n'):
            raise RuntimeError('Incomplete final CSV row')
        after_camera = camera(con, args.cap)
        after_conditions = conditions()
        if before_conditions != after_conditions:
            raise RuntimeError('Conditions changed')
        playback_seconds = (pause_received - resume_acknowledged) / 1e9
        if not 19.5 <= playback_seconds <= 20.5:
            raise RuntimeError("Observed playback duration disagrees with 20 seconds")
        log = new[0]
        data = rows(log)
        frames = []
        audited_rows = []
        previous = None
        guard = 250_000_000
        for i, row in enumerate(data):
            elapsed = int(row["elapsed"])
            dt = float(row["frametime"])
            if previous is not None and elapsed <= previous:
                raise RuntimeError("Frame timestamps are not increasing")
            previous = elapsed
            if not math.isfinite(dt) or dt <= 0:
                raise RuntimeError("Bad frame interval")
            # Keep only complete intervals whose possible clock bounds are inside the guard.
            keep = (i > 0 and epoch_lo + elapsed - dt * 1e6 >= resume_acknowledged + guard
                    and epoch_hi + elapsed <= pause_received - guard)
            audited_rows.append([elapsed,row['frametime'],int(keep)])
            if keep:
                frames.append((elapsed, dt))
        if len(frames) < 100:
            raise RuntimeError("Too few guarded frames")
        durations = [x[1] for x in frames]
        ordered = sorted(durations)
        measured_ms = (frames[-1][0] - frames[0][0]) / 1e6
        if not 19_300 <= measured_ms <= 19_600:
            raise RuntimeError('Selected duration outside declared 19.3 to 19.6 second range')
        sum_after_first_ms = sum(durations[1:])
        if abs(measured_ms - sum_after_first_ms) > max(2, measured_ms * 0.002):
            raise RuntimeError("Frame sums disagree with elapsed time")
        report = {
            "demo_ticks": [1280,2560], "server_ticks": [19436,20716],
            "helper_sha256":helper_sha,
            "raw_csv_sha256":hashlib.sha256(last_contents).hexdigest(),
            "raw_csv_stable_seconds":1,
            "logger_stop_acknowledged":True,
            "maximum_epoch_uncertainty_ms":50,
            "required_selected_duration_seconds":[19.3,19.6],
            "camera_before": before_camera, "camera_after": after_camera,
            "frame_cap": args.cap, "raw_rows": len(data), "kept_rows": len(frames),
            "conditions_before":before_conditions,"conditions_after":after_conditions,
            "selection_lower_elapsed_ns":resume_acknowledged + guard - epoch_lo,
            "selection_upper_elapsed_ns":pause_received - guard - epoch_hi,
            "guard_ms_each_end": 250, "logger_epoch_uncertainty_ms": (epoch_hi-epoch_lo)/1e6,
            "resume_to_pause_observed_seconds": (pause_received-resume_sent)/1e9,
            "resume_ack_to_pause_observed_seconds": playback_seconds,
            "requested_demo_timescale": 1,
            "playback_speed_check": "1280 ticks at 64 Hz completed within 20 +/- 0.5 seconds",
            "boundary_limit": "The end guard assumes pause notification delay is below 250 ms; this is not a proven bound",
            "measured_seconds": measured_ms/1000,
            "sum_after_first_seconds": sum_after_first_ms/1000,
            "median_present_interval_ms": statistics.median(durations),
            "p95_present_interval_ms": ordered[math.ceil(0.95*len(ordered))-1],
            "throughput_fps": (len(frames)-1)/(measured_ms/1000),
            "input_latency_measured": False, "display_latency_measured": False,
            "present_intervals_are_gpu_execution_time": False,
            "measurement": "Instrumented present-call intervals selected using guarded real-time SourceTV playback notifications",
        }
        shutil.copyfile(log, args.out / "frames.csv")
        if hashlib.sha256((args.out/'frames.csv').read_bytes()).hexdigest() != report['raw_csv_sha256']:
            raise RuntimeError('Frame log changed while copying')
        with (args.out/'samples.csv').open('w',newline='') as stream:
            writer = csv.writer(stream)
            writer.writerow(['elapsed_ns','interval_ms','selected'])
            writer.writerows(audited_rows)
        (args.out / "summary.json").write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps(report), flush=True)
    finally:
        if control is not None:
            try:
                control.sendall(b":logging=0;")
            except OSError:
                pass
            control.close()
        (args.out / "console.txt").write_text(con.text)
        con.s.close()


if __name__ == "__main__":
    main()
