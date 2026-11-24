# DistributedGaming

Use several PCs to share selected game work. Rust runs the harness. A game adapter will capture work, send it to workers, and check the result.

## Current state

The Rust harness runs a fixed CPU ray-tracing test scene across TCP workers. It checks every output byte against a local full-frame render. It does not capture, patch, or run a game. It does not use the GPU yet.

The first test used five physical PCs over direct Tailscale paths on a wired network. Each active wired link reported 1 Gb/s. All 24 frames passed exact pixel checks. Distributed work was slower in both tests. See the [test record](docs/validation/2026-11-24.md).

## Goal and progress

Run one supported game across at least two physical PCs. Use the same settings and image quality. Beat the fastest participating PC running alone. Report frame time and input delay separately.

<!-- progress:start -->
Goal progress: **20%** (20/100 accepted milestone points).

This is a checklist score. It is not an estimate of time, effort, or game speedup.

| Step | Acceptance area | Points | State |
| --- | --- | ---: | --- |
| M1 | Rust harness and failure tests | 10 | complete |
| M2 | Transport on physical PCs | 10 | complete |
| M3 | Selected game baseline | 10 | pending |
| M4 | Capture and replay game work | 20 | pending |
| M5 | Split one game pass | 15 | pending |
| M6 | Game correctness and recovery | 15 | pending |
| M7 | Measured game benefit | 10 | pending |
| M8 | Repeatable supported release | 10 | pending |
<!-- progress:end -->

[progress.json](progress.json) contains the fixed acceptance checks. A milestone receives its points only when the checks pass and evidence is recorded. Every PR must update this README. The percentage may stay the same. CI checks the README change, score, evidence paths, and fixed plan.

## Build

Use Rust 1.93 or later. CI uses Rust 1.93.1 on Linux and Windows. Python 3.10 or later runs the progress checks.

```sh
cargo build --release --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
python3 scripts/check_progress.py
python3 scripts/test_progress.py
```

On Windows, use `python` and `target\release\dg.exe`.

## Run a local transport test

1. Open a terminal. Start a worker:

   ```sh
   target/release/dg worker --listen 127.0.0.1:47000 --jobs 12
   ```

2. Open a second terminal. Run the test:

   ```sh
   target/release/dg bench --worker 127.0.0.1:47000 --frames 10 --warmup 2
   ```

3. Check the JSON output. `verified_frames` must be 12. `network_scope` must be `loopback_only`. A failed benchmark returns a nonzero exit code. The worker logs a failed connection and continues until its connection limit. It stops after 12 connections in this example.

This test uses one PC. It does not prove LAN scaling.

## Run on several PCs

1. Copy the same release binary to each compatible PC. Check the SHA-256 hash on each PC.
2. Start each worker on its private address:

   ```sh
   target/release/dg worker --listen WORKER_IP:47000 --jobs 12 --timeout-ms 10000
   ```

3. Create a `runs` folder if it does not exist. Run the coordinator. Add one `--worker IP:PORT` for each worker:

   ```sh
   target/release/dg bench --worker WORKER_IP:47000 --frames 10 --warmup 2 --timeout-ms 10000 --out runs/run-01.json
   ```

4. Check all pixels and timings. The coordinator also renders one tile. Keep raw reports private: they contain worker addresses. Use a new report filename for each run.

The wire protocol has no built-in authentication or encryption. Use trusted peers on a private network, Tailscale, or SSH tunnels. The default worker binds to loopback. The harness does not change firewall rules. A worker accepts only the fixed scene and tile bounds. It accepts no code, commands, shaders, or file paths.

## Read the result

- `local_parallel` uses the same number of tiles and threads as the distributed run has execution slots. It is a harness baseline, not the fastest-PC game baseline.
- `distributed` includes thread creation, TCP connection, computation, transfer, and image assembly.
- Reference rendering, exact comparison, and report writing are outside the timed interval. Run order alternates.
- `local_parallel_over_distributed_median` is above 1 only if the distributed median is lower. It does not measure game FPS.
- `physical_host_count_verified` stays false in the automatic report. IP addresses alone cannot prove separate hardware. The test record supplies the manual host check.
- `game_speedup_proven` stays false. No game benchmark has passed yet.
- Ten samples are a smoke test. Do not use them for a broad performance claim. `p95` uses nearest rank.
- Network reads and writes share an absolute job deadline. CPU rendering is bounded by ray count; a socket deadline does not preempt CPU work.

## First game

Half-Life 2 is the first capture and replay target. Its demo system supports repeatable scenes. It was confirmed in the user's Steam library. No game files, account IDs, or full library lists are stored here.

Baldur's Gate 3 is a later Vulkan candidate. Selection is not integration. No game has been downloaded to the test nodes, captured, patched, or benchmarked by this project. See the [game selection record](docs/game-selection.md).

## LLM-assisted reversing

An LLM may help find a pass, trace dependencies, and propose a patch. Rust will own dispatch, resource bounds, replay checks, and measurements. The first PR defines the [profile contract](docs/game-profile.md); it does not call a model or execute a generated patch.

An unknown dependency blocks a patch. A plausible explanation or valid schema is not proof. Read the [research method](docs/research.md) and [detractor record](docs/reviews/0001.md).

## Latest change

- Added the Rust worker, coordinator, fixed test scene, strict wire checks, and JSON reports.
- Added local parallel baselines and exact full-frame comparisons.
- Added failure tests, Linux/Windows CI, and the mandatory README progress check.
- Ran two cases across five physical PCs. Both passed pixel checks and both were slower than local work.
- Completed two detractor rounds. Fixed the progress reweighting gap and added a corrupt-pixel test.
- Passed 16 Rust tests and 6 Python tests on Linux and Windows CI. Closed M1 and M2. Goal progress is 20% by the fixed checklist.

Next: install and pin the Half-Life 2 build on one test PC. Record an offline demo. Capture one frame and measure each pass. Select a pass only when its measured compute saving can exceed its transfer and merge cost. Keep M3 pending until that evidence exists.
