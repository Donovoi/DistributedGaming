# Original shader with fixed inputs

Date: 2026-11-24. Location: remote PC A. Goal progress: 30%. M4 remains pending.

The original CS2 reduction shader ran in isolation on two captured input sets. Each set produced the same output bits in three runs with different output starting values. All input bytes and guards stayed unchanged. Core and synchronization validation reported no errors, including teardown.

The [machine record](cs2-native-oracle-2026-11-24.json) contains file identities, the execution contract, and all six run results. The [tool procedure](../../tools/native-oracle/README.md) gives the build and run commands. Original shader bytes, capture files, and game resources remain private.

## Method

Hypothesis: the original shader can run with fixed supplied buffers and provide a reference that does not depend on a replay of earlier game work.

1. Pin the shader by SHA-256. Validate its unchanged SPIR-V with `spirv-val --target-env vulkan1.2`. Inspect its capabilities, entry point, execution mode, and buffer declarations.
2. Check the selected shader and its pipeline state in both captures. Both use `MainCs`, one group of 256 threads, storage texel bindings 159 and 160, and uniform binding 2. No specialization values, push constants, or required subgroup size were recorded.
3. Use the original first fixture from the earlier Rust experiment. Use a different histogram from the second captured scene. Freeze those bytes. Do not take a new replay snapshot during an oracle run.
4. Upload those bytes into separate buffers. Execute the original shader. Read the output after the fence completes. Check three output starting values, the complete input buffers, and guard bytes outside the output view.
5. Compare the isolated output with the recorded reference bits. The first fixture gives 1041600569. The second fixture gives 1034300058. Both match the sampled references. These isolated runs provide new oracle outputs; their input and output bytes share one execution.

The second scene uses a different camera direction and a different histogram. Its input contains 168 nonzero bins and a weighted sum of 79,116,069. The first frozen fixture contains 207 nonzero bins and a weighted sum of 94,448,137. Both contain 921,600 histogram samples. These counts do not prove the game's internal rendering resolution.

## Checks and limits

The remote build used warnings as errors. Seven invalid-input checks passed before GPU setup. The real GPU runs used an RTX 3070 and driver 615.71.09. The GPU workload was restored after each test burst. All tests and builds ran remotely or in CI VMs.

RenderDoc endpoint reads remain separate prefix replays. One earlier comparison from the second capture failed because its input snapshots differed. Later first-capture replays also produced different histogram bytes. These observations do not invalidate the saved fixed inputs, but they prevent a claim that whole-capture replay is deterministic. Do not replace the frozen fixtures with a newly replayed snapshot and assume it is the same case.

The isolated buffers cannot alias. This does not prove that original game allocations lacked aliases. The original producer/consumer barriers have not been accepted as a complete game patch contract. Full-frame correctness remains open. The earlier scalar reconstruction still fails by one float step.

The Rust reconstruction has not yet been checked on the second fixture or on arithmetic edge cases. These gaps keep M4 pending. This tool has no network path, no game hook, and no speed result. The selected pass is too small to justify the current full-input LAN design.

Next: compare the unchanged WGSL calculation with this oracle on the second fixture and on bounded arithmetic edge cases. Continue looking for game work expensive enough to cover transfer and merge costs.

Sources: [Vulkan texel-buffer views](https://docs.vulkan.org/refpages/latest/refpages/source/VkBufferViewCreateInfo.html), [Khronos synchronization examples](https://github.khronos.org/Vulkan-Site/guide/latest/synchronization_examples.html), and [wgpu 26.0.1 binding types](https://docs.rs/wgpu/26.0.1/wgpu/enum.BindingType.html).
