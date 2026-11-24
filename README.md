# DistributedGaming

Use several PCs to share selected game work. Rust runs the harness. A game adapter will capture work, send it to workers, and check the result.

## Current state

The Rust harness runs a fixed CPU ray-tracing test scene across TCP workers. It checks every output byte against a local full-frame render. It does not capture, patch, or run a game. Separate tools have now captured a Counter-Strike 2 world frame on a remote NVIDIA GPU. Replay produced a visible scene, but repeated replay did not match every byte. See the [game capture record](docs/validation/cs2-capture-2026-11-24.md). No game work is distributed yet.

The first test used five physical PCs over direct Tailscale paths on a wired network. Each active wired link reported 1 Gb/s. All 24 frames passed exact pixel checks. Distributed work was slower in both tests. See the [test record](docs/validation/2026-11-24.md).

A separate [Rust GPU probe](tools/reduction-probe/README.md) now matches one captured compute output with a WGSL reimplementation. It accepts only one private case. It does not patch the game or send work to another PC. M4 remains pending.

## Goal and progress

Run one supported game across at least two physical PCs. Use the same settings and image quality. Beat the fastest participating PC running alone. Report frame time and input delay separately.

<!-- progress:start -->
Goal progress: **30%** (30/100 accepted milestone points).

This is a checklist score. It is not an estimate of time, effort, or game speedup.

| Step | Acceptance area | Points | State |
| --- | --- | ---: | --- |
| M1 | Rust harness and failure tests | 10 | complete |
| M2 | Transport on physical PCs | 10 | complete |
| M3 | Selected game baseline | 10 | complete |
| M4 | Capture and replay game work | 20 | pending |
| M5 | Split one game pass | 15 | pending |
| M6 | Game correctness and recovery | 15 | pending |
| M7 | Measured game benefit | 10 | pending |
| M8 | Repeatable supported release | 10 | pending |
<!-- progress:end -->

[progress.json](progress.json) contains the fixed acceptance checks. A milestone receives its points only when the checks pass and evidence is recorded. Every PR must update this README. The percentage may stay the same. CI checks the README change, score, evidence paths, and fixed plan.

## Work rules

Make small, complete commits. Include the README and other documentation needed for each change. Push promptly. Merge task-owned PRs as soon as both detractor rounds and the required checks pass for the current head and relevant base. Recheck after either changes. Standing permission to merge is recorded in [AGENTS.md](AGENTS.md). Required checks and branch rules still apply.

Keep the user's PC available. Run all checks, builds, installs, tests, benchmarks, and game work on remote PCs or in VMs. This includes documentation checks and both the coordinator and worker. Prefer remote PCs or GitHub-hosted CI VMs. A local VM needs CPU, memory, and I/O limits that keep the host usable. WSL alone is not enough. Local source reading, light edits, and Git operations are allowed.

Record where each check ran, which commit it tested, and the result. Keep private addresses out of public records. Earlier test records keep their original dates and locations.

## Build

Run these commands on a remote PC or in a VM that meets the work rules above. Use Rust 1.93 or later. CI uses Rust 1.93.1 on Linux and Windows. Python 3.10 or later runs the progress checks.

```sh
cargo build --release --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
python3 scripts/check_progress.py
python3 scripts/test_progress.py
```

On Windows, use `python` and `target\release\dg.exe`.

## Run a loopback transport test

Use one remote PC or a VM that meets the work rules above. Run both terminals there.

1. Open a terminal on that test host. Start a worker:

   ```sh
   target/release/dg worker --listen 127.0.0.1:47000 --jobs 12
   ```

2. Open a second terminal. Run the test:

   ```sh
   target/release/dg bench --worker 127.0.0.1:47000 --frames 10 --warmup 2
   ```

3. Check the JSON output. `verified_frames` must be 12. `network_scope` must be `loopback_only`. A failed benchmark returns a nonzero exit code. The worker logs a failed connection and continues until its connection limit. It stops after 12 connections in this example.

This test uses one host. It does not prove LAN scaling.

## Run on several PCs

Run the coordinator and all workers on remote test PCs. Keep the user's desktop free of test work.

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
- `game_speedup_proven` stays false. No distributed game result has proved a benefit.
- Ten samples are a smoke test. Do not use them for a broad performance claim. `p95` uses nearest rank.
- Network reads and writes share an absolute job deadline. CPU rendering is bounded by ray count; a socket deadline does not preempt CPU work.

## First game

Counter-Strike 2 is the current test game. Its installed Linux binary is 64-bit. A local SourceTV demo ran with `-insecure` and was captured through Vulkan. The capture contains world geometry. This is a spectator scene on an isolated display. It does not measure player input or display delay. No game files, account IDs, or full library lists are stored here.

The installed Linux builds of Half-Life 2 and Tomb Raider are 32-bit. They do not fit the current Linux RenderDoc capture route, which requires x86-64. This does not rule out other capture methods. Remote Steam sign-in and the [CS2 spectator baseline](docs/validation/cs2-baseline-2026-11-24.md) are complete. Verified pass replay is still pending. See the [game selection record](docs/game-selection.md).

## LLM-assisted reversing

An LLM may help find a pass, trace dependencies, and propose a patch. Rust will own dispatch, resource bounds, replay checks, and measurements. The first PR defines the [profile contract](docs/game-profile.md); it does not call a model or execute a generated patch.

An unknown dependency blocks a patch. A plausible explanation or valid schema is not proof. Read the [research method](docs/research.md) and [detractor record](docs/reviews/0001.md).

## Latest change

- Corrected the scope of the native reference snapshots. RenderDoc starts a replay prefix when an event is selected. Matching snapshots on either side of a pass do not prove input immutability within one execution.
- Rejected one comparison from a second capture after its input snapshots differed. That attempt is not part of the fixed GPU fixture. The first fixture's hashes and exact GPU output match remain valid.
- Kept the Rust probe's isolated input and guard checks separate from the capture replay checks. No runtime code or measured bytes changed.
- Recorded the [correction and evidence](docs/validation/cs2-reduction-2026-11-24.md) and [detractor review](docs/reviews/0008.md). Required CI checks must pass before merge.
- Goal progress stays at 30%. M4 remains pending. No game work is distributed and no speedup is proved.

Next: run the original shader in isolation with fixed inputs. Complete the pass execution contract, check a different captured input, and test arithmetic edge cases. Keep the full-frame replay mismatch open. Look for work expensive enough to cover transfer and merge cost; this small reduction has no demonstrated LAN benefit.
