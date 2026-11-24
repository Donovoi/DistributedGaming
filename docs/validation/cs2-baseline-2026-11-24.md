# Counter-Strike 2 spectator baseline

Date: 24 November 2026. Location: remote PC A. Repository base: `9a2c8d9fa9356ee080bef6901e938a0580d2ecbc`.

## Result

One warmup and three measured runs completed with the same game build, demo, saved settings, camera endpoints, and window size. M3's game access, pinned baseline, and real-frame capture checks are complete. M4 remains pending because no selected game pass has verified output. Goal progress is 30%.

This is a headless SourceTV spectator rendering baseline. It is not a player input test. The PC has an Intel Core i7-10700F and an RTX 3070 with driver 615.71.09. It runs Ubuntu 26.04.1. It has not been established as the fastest participating PC.

| Run | Selected samples | Selected time | Median interval | p95 interval | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| Warmup, excluded | 2,814 | 19.487 s | 6.94005 ms | 7.88663 ms | 144.352 frames/s |
| 1 | 3,387 | 19.493 s | 5.75097 ms | 6.92451 ms | 173.700 frames/s |
| 2 | 3,348 | 19.501 s | 5.82394 ms | 6.97475 ms | 171.633 frames/s |
| 3 | 3,333 | 19.494 s | 5.84320 ms | 7.00282 ms | 170.925 frames/s |

These intervals come from the instrumented present-call path. They are not GPU execution times or display times. Input delay and display delay were not measured. Three runs describe this scene; they do not establish broad game performance or a confidence bound for a speedup.

The sample selector uses a 250 ms guard at each end of playback notifications. The end guard assumes notification delay stays below 250 ms. This bound was not proved. The record does not prove that every selected interval falls before the actual pause. Logger-clock uncertainty was 4.33 to 4.69 ms in the three measured runs.

## Fixed inputs and checks

The [machine record](cs2-baseline-2026-11-24/summary.json) stores game build 26879581, the executable and demo hashes, saved graphics settings, tool identity, and every run's checks. The saved settings include MSAA 4, vsync 0, and the raw FSR setting 0. The window was 1920 by 1080 before and after every run. The requested size in the launch and config was 1280 by 720; borderless mode used the display size. Internal render scale was not separately measured. Do not infer native scene resolution from window size alone.

The active demo reported `de_dust2`, patch 14201, build number 10955, and server start tick 18156 before and after each run. Its file hash matched the pinned recording. TrueView prediction was disabled. The auto director was disabled. The camera readback stayed at x 182, y 2439, z -39.984375, pitch 0, and yaw -4 at both endpoints. Endpoint checks are not continuous camera observation.

Each measured playback covered relative demo ticks 1280 through 2560, at requested speed 1. The corresponding server ticks were 19436 through 20716. Observed playback took about 20.01 seconds, within the declared 20 +/- 0.5 second check. Frame cap 0 was read back from the game. The saved video-config hash and expected executable hash matched before and after every run. MangoHud was loaded; RenderDoc was absent from the game process maps.

The [earlier capture](cs2-capture-2026-11-24.md) uses the same game build, demo hash, map, and camera. It shows real world geometry and a 1920 by 1080 present image. It is a separate capture run. Its exact tick and saved video-config hash at capture time were not recorded. It does not prove pixel equality with the timed runs. Its repeat-replay mismatch remains open under M4.

The remote inference worker was active before the burst, inactive before each run and after the last run, and restored to active afterwards. The restore timer did not fire during measurement. The isolated display was stopped after the checks. No benchmark or validation ran on the user's desktop.

## Measurement method

MangoHud came from source archive SHA-256 `a2e5c72b373645823b48624dd03d1852a250acbeeec305b11c661612f6e0dc93`, tag `v0.6.9-1`. It was built in the pinned sniper SDK to run in the game's runtime. The machine record lists the build options and library hash. NVML was disabled; GPU utilization fields are not evidence. Build dependencies are not fully pinned, so this is not a released toolchain.

The HUD was hidden. Logging used `log_interval=0`, which records each sampled frame interval, and a private tmpfs output folder. Uploads were disabled. Logging and console control add overhead; that overhead was not measured separately. Any later comparison must use the same measurement setup.

The [collector](../../tools/cs2/benchmark_demo.py) uses the same `CLOCK_MONOTONIC_RAW` clock as this MangoHud version. It bounds the logger start time from the start request and the first observed row. Before these runs, the acceptance limits were set to at most 50 ms clock uncertainty and 19.3 to 19.6 seconds of selected data. All four runs met those limits.

The collector waits for `LoggingFinished`, then checks that the CSV is unchanged for one second. It checks the copied file hash. It rejects nonpositive intervals, repeated timestamps, changed run conditions, wrong playback duration, and inconsistent timing sums. Its source hash is saved in each run.

Each public CSV contains only elapsed nanoseconds, interval milliseconds, and a selection flag. Raw hardware headers and console text stay private. All exported values were compared with the original private CSVs on the remote PC. Selection flags, row counts, median, p95, and throughput were recomputed independently during export. The p95 uses nearest rank. Throughput uses N minus 1 intervals divided by the elapsed time from the first selected sample to the last; it is not an average of instantaneous FPS.

## Repeat the procedure

Use a remote Linux PC or a suitable VM. This procedure needs the pinned private demo and the tested container layout under `/work`. The collector is an experiment for this profile, not a general game launcher.

1. Record the game, demo, runtime, graphics settings, and tool hashes. Reject an unexpected build or file. Keep Steam account files outside the repo.
2. Record the original GPU service state. Set a restore timer, then stop the service for the test. Start a separate X display with enough time for the whole series. Check that Steam is signed in. For this display, the Steam client needed `SDL_VIDEO_X11_XRANDR=0` to create its window; the game launch did not use this override.
3. Start the game through the sniper runtime with `-steam -insecure -novid -windowed -w 1280 -h 720 -netconport 21234 -condebug -con_logfile UNIQUE_PRIVATE_LOG +fps_max 0 +playdemo dg_m3_tv`. Use a new console log. Select only the pinned MangoHud implicit-layer manifest. Use the MangoHud configuration in the machine record. Disable the RenderDoc layer.
4. Wait for `Host activate: Playing Demo` in that new log, then allow five seconds to settle. Stop if launch reports a fatal error. Send `demo_pause` through the private console port. Do not seek while the world is still loading.
5. Run `python3 tools/cs2/benchmark_demo.py NEW_PRIVATE_OUTPUT` inside the test container. The collector seeks to relative tick 640, applies the spectator settings, and runs ten seconds to tick 1280 before measurement. It resumes, pauses at tick 2560, and records the guarded interval. Use a new output directory each time.
6. Run the whole collection once for warmup, then three more times for measurement. Do not change the helper during the series. Check that the GPU service remains stopped before every run and after the last run.
7. Stop the game. Restore the service to its original state and verify it. Stop the test display. Retain raw evidence privately.
8. On a remote PC or CI VM, run `python3 scripts/check_cs2_baseline.py` to recompute the published record. This checker does not recreate the private scene or certify a new benchmark run.

The failed launch, wrong frame-cap trial, incomplete condition checks, and logger-control trial are excluded. Their private files were retained. The accepted series above used one unchanged collector and one game process.

Sources: [MangoHud frame updates](https://github.com/flightlessmango/MangoHud/blob/v0.6.9-1/src/overlay.cpp), [logging and completion response](https://github.com/flightlessmango/MangoHud/blob/v0.6.9-1/src/logging.cpp), [raw clock](https://github.com/flightlessmango/MangoHud/blob/v0.6.9-1/src/mesa/util/os_time.c), and [SDL display hint](https://wiki.libsdl.org/SDL3/SDL_HINT_VIDEO_X11_XRANDR).
