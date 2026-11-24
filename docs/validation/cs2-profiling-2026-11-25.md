# CS2 profiling and candidate check

Date: 2026-11-25. Location: remote PC A. Repository base: `9b907de0bf5fa2a10a74d6e351e65c51e913ef39`. Private profiling and analysis helpers are outside that commit. Their hashes are in the machine record. Progress stays at 50%. M5 remains pending.

Scene and physics code remain prominent in a CPU-only sample of the fixed demo. Inspect these areas for a bounded job next. Neither is a proven bottleneck or a safe remote job. Reject both GPU traces for timing claims. No game work was patched or distributed.

## Reason for this check

The [selected reduction](cs2-boundary-map-2026-11-24.md) took 3.072 microseconds in one earlier sample. Sending only a 512-byte half input over 1 Gb/s takes at least 4.096 microseconds of payload time. Readback, latency, remote work, and return traffic add more time. Keep this pass as a small replay check. Find work with a credible cost margin before expanding its descriptor tracking.

Hypothesis: a CPU/GPU trace can identify larger work groups in the fixed CS2 scene. Result: the GPU timing was not usable. CPU samples provide a shortlist, with the limits below.

## Run conditions

All game runs, exports, and analysis ran on remote PC A. Nsight Systems version was `2025.5.2.266-255236693005v0`. The game ran as the normal test user, through its Steam runtime, on the isolated display. It used `-insecure` and the same offline SourceTV scene as M3. No matchmaking was used.

The executable, demo, video configuration, camera, and effective 1920 by 1080 window passed the existing before/after checks. The [machine record](cs2-profiling-2026-11-25.json) pins their hashes. The window size does not establish internal render scale. Playback covered server ticks 19436 through 20716 at normal speed. This is a spectator scene. It measures no player-input or display delay.

Each completed run stopped the remote GPU workload under a restore timer. Cleanup restored that workload. The timer did not fire. This checks manual cleanup; it does not prove automatic recovery after a controller crash.

| Run | Collection | Guarded intervals | Median ms | p95 ms |
| --- | --- | ---: | ---: | ---: |
| 04 | CPU plus Vulkan batches | 2,787 | 7.014100 | 8.350110 |
| 05 | CPU plus individual Vulkan work | 2,806 | 6.915435 | 8.383960 |
| 07 | CPU samples and scheduling only | 2,956 | 6.614805 | 7.779440 |

These are instrumented present-call intervals. They are not GPU execution times. Each row is one diagnostic run. Different collection costs and changing sample counts prevent a speedup claim. No adjacent unprofiled control or repeated performance comparison was made in this slice.

## Rejected GPU evidence

Run 04 contains 5,430 GPU batch records. Some spans reach 11.218 seconds while the game continues to present frames. Another 2,362 spans are exactly one nanosecond. The record pairing is not credible for this scene. Do not repair the times by assuming a missing-record pattern. The cause remains unproved.

Run 05 contains 146,538 GPU records. Its longest recorded range is 1.437 milliseconds. Plausible ranges alone do not prove coverage. The profiler reports that its limit of 8,192 Vulkan GPU event timestamps per frame was exceeded by 140,920, and excess events were not captured. Do not use this trace to calculate full GPU occupancy, overlap, or critical-path time.

NVIDIA also documents that compute and transfer command-buffer ranges can end too early on the timeline. This is a separate limit; it does not explain away the defects above. See the [Vulkan GPU trace notes](https://docs.nvidia.com/nsight-systems/UserGuide/index.html#vulkan-gpu-trace-notes).

## CPU observations

Run 07 disabled Vulkan, OSRT, and NVTX tracing. CPU sampling and scheduling used process-tree scope. The recorded event was Reference Cycles with a 2,000,000-cycle period and DWARF backtraces. Four instruction samples were collected per backtrace. The export contains 90,435 instruction samples and 934,853 scheduling records across the whole trace. It has no API or GPU-workload tables and reports no diagnostic warnings or errors. These checks do not prove that every short-lived thread was sampled.

The guarded CPU window contains 82,380 leaf samples from the verified CS2 process. Count one instruction address per sample. Do not count each caller in a stack as another sample. The table retains all samples in its denominator.

| Module | Run 05 leaf samples | Run 07 leaf samples |
| --- | ---: | ---: |
| Scene system | 12,502 | 12,021 |
| Physics | 11,119 | 11,426 |
| Vulkan renderer | 11,166 | 10,738 |
| NVIDIA graphics driver | 9,565 | 8,980 |
| All modules and kernel | 98,200 | 82,380 |

The two windows are about 19.51 seconds each. Raw counts are exploratory. They do not measure time saved by removing tracing. Scene and physics samples occur on the game's worker threads. That supports inspecting these routines. It does not show their dependencies, data size, or effect on frame completion. CPU work can include waiting and synchronization.

The profiler did not resolve function names for 82,319 of run 07's guarded samples. Its saved game-module paths stop at the first space. Module attribution therefore uses each sample's original instruction address and the saved process memory map. Preserve spaces when reading map paths. Match an address to its containing executable mapping; keep kernel and unknown addresses separate. Run 07 has 42 kernel samples and no unmatched user addresses.

Run 07 saved maps before and after playback. Mappings used by sampled user addresses agree at both endpoints. One new render thread appears in the ending snapshot with zero recorded CPU ticks. Endpoint maps do not rule out a transient unload or address reuse during playback. The 58 executable game modules were hashed after playback, while paused. The mapped inode matched the file, and its size, inode, and modification time stayed equal during hashing.

Function-level reversing must use the ELF load layout to convert a file offset to a virtual address. A nearby exported name is not a function boundary. Require a defined function with a nonzero range that contains the address; keep aliases and unknown ranges explicit. See the [ELF symbol rules](https://gabi.xinuos.com/elf/05-symtab.html).

## Time and evidence limits

Runs 05 and 07 saved raw-monotonic, monotonic, and wall-clock anchors. The CPU window uses monotonic time from resume acknowledgement plus 250 ms through pause receipt minus 250 ms. Subtract the trace's monotonic start to query sample times. This is not exact alignment with each selected frame-log row. The end guard assumes that pause notification arrives within 250 ms; that bound remains unproved.

Run 07's paired clock reads took at most 9.896 microseconds. The raw-minus-monotonic offset changed by 342.801 microseconds across playback. Do not use one fixed offset to align the entire run. Run 04 lacks the required playback anchors, so its whole-trace CPU counts are not compared with guarded frame counts.

A failed run 06 called the older run 05 controller. It stopped before collection, restored the GPU workload, and overwrote run 05's starting map, process snapshot, start diagnostics, and phase log. Run 05's raw trace, frame files, playback anchors, ending map, and module identities remain intact. Its published CPU counts were recomputed using only the ending map. There is no reproducible two-map claim for run 05. Run 07 checks its script paths before stopping the workload and refuses to overwrite controller evidence.

Raw profiler exports contain environment data, paths, and account details. Keep them private. The public machine record contains selected aggregates and artifact hashes only. A hash identifies evidence; it does not prove the measurement is correct. The independent reviewer inspected the private source records.

## Decision and next checks

1. Inspect a specific scene or physics worker routine. Establish its entry and exit, duration, reads, writes, and ownership. Module sample share alone is not enough.
2. Measure the input bytes that change per job, the result size, and the time before the result is needed. Cached static data does not remove changing state.
3. Compare saved work with readback, transport, remote execution, result transfer, and synchronization. At 1 Gb/s, 100 KiB takes at least 0.8192 ms and 1 MiB takes at least 8.388608 ms in one direction, before other costs.
4. Reject a candidate with unknown shared writes or no credible time margin. Use a local replacement and exact result checks before a remote replacement.
5. Use repeated runs and an adjacent unprofiled control before any performance claim. Keep present time, CPU/GPU execution, and input delay separate.

The [Nsight collection guide](https://docs.nvidia.com/nsight-systems/UserGuide/index.html) describes CPU sampling separately from API tracing. The [two detractor rounds](../reviews/0013.md) record the design and evidence challenges. No milestone acceptance check closed.
