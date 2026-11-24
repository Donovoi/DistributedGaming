# First reconstructed game-pass check

Date: 24 November 2026. Location: remote PC A. Repository base: `3b7cc99ad67eb0bb9955b191e7c85d5b90395f67`.

## Result

A Rust and WGSL reimplementation reproduced one captured game's four-byte compute output on an RTX 3070. It passed three runs with different initial output values. All input bytes and both guard words stayed unchanged. This is a fixed-case experiment. It does not complete M4. Goal progress stays at 30%.

The [machine record](cs2-reduction-2026-11-24.json) contains the tested source, lockfile, binary, inputs, and shader hashes. The [tool procedure](../../tools/reduction-probe/README.md) explains its fixed input gate and output checks. Raw game resource snapshots, the original shader binary, and captures remain private.

## Hypothesis and method

Hypothesis: inspecting one captured shader's instructions is enough to write a separate GPU calculation that matches its output for the captured inputs. This is a small test of instruction-guided reversing. It does not show that a whole game can be divided or that a model saves reversing time.

1. Inspect the 19 compute dispatches in the existing CS2 capture. Select event 6905 as a small correctness exercise. It uses one workgroup of 256 threads. Its typed buffers contain a 256-bin histogram and one floating-point output. A 32-byte uniform range supplies the parameters.
2. Open the unmodified capture with a fresh RenderDoc controller. Read the histogram, uniform range, and old output immediately before E6905. Execute through E6905. Read its output and inputs again. Repeat with another fresh controller. Both sessions produced the same input hashes and native output bits. The dispatch changed the output and did not change either input range.
3. Read the captured shader instructions. The reduction weights each histogram count by its bin index and sums with wrapping 32-bit integer arithmetic. It converts the result to a 32-bit float, normalizes it, applies the captured logarithmic range, and computes base-two exponentiation. Preserve the normalization constant's exact bits, `0x3b808081`.
4. Check a separate scalar reconstruction with explicit float32 rounding and `exp2f`. It produced bit pattern 1041600570. The native replay produced 1041600569. Record the one-step difference as a failed exact check. This difference alone does not establish that the native shader is wrong.
5. Run the embedded WGSL reimplementation through Rust and `wgpu` 26.0.1. Use the native replay output as the reference. Start the output word with 0, 4294967295, and 305419896 in separate runs. Check all four output bytes, all 1,056 input bytes, and guard words on both sides of the output.

All three GPU runs produced 1041600569. The reported adapter was NVIDIA GeForce RTX 3070, backend Vulkan, driver 615.71.09. The tool rejects CPU and software adapters. It does not load the original SPIR-V binary; its result is a checked reimplementation of one case.

The reimplementation remaps descriptor bindings. The original set 0 bindings 159, 2, and 160 become WGSL group 0 bindings 0, 1, and 2 for the histogram, uniform, and output. Typed buffer access becomes storage-buffer access. The native output occupies one four-byte word. The probe writes the middle word of a three-word buffer so it can check a guard on each side. These are changes in the test implementation, not a patch applied to the game.

This pass's uniform data contains dimensions 1280 by 720. Those values do not establish presentation resolution or the timed baseline's internal image quality. The separate captured present image is 1920 by 1080.

## Checks and limits

Formatting, Clippy with warnings denied, the three CPU input tests, and the release build passed on the remote PC. The final tested binary hash is in the machine record. The GPU run used an outer 45-second process timeout. The competing GPU worker was stopped under a restore timer, then restored and confirmed active. Final-head CI must pass on Linux and Windows before merge. CI checks the source and input tests; it does not run the private GPU case.

The native reference is an output of unmodified capture replay. It is not a separate live-game readback. The earlier whole-frame mismatch stays open. This check does not establish a complete descriptor, specialization, push-constant, alias, or synchronization contract. A different captured input and arithmetic edge cases have not been checked. Those gaps keep M4 pending.

One earlier RenderDoc counter reported about 0.003072 ms for the original dispatch. This is one profiling observation, not a timing distribution for this tool. Sending the 1,024-byte histogram alone over a 1 Gb/s link takes at least 0.008192 ms to serialize, before protocol, queue, copy, and return costs. The current full-input design has no demonstrated saving. Do not admit it as a game patch or call this a speedup.

Next, extract a second real input without changing the implementation. Complete the execution contract and test arithmetic edge cases against the native shader. Keep the exact comparison. Continue looking for more expensive work that can cover the network cost.

Sources: [SPIR-V instruction rules](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html), [WGSL floating-point limits](https://www.w3.org/TR/WGSL/#floating-point-evaluation), and [the pinned wgpu package](https://github.com/gfx-rs/wgpu/blob/v26.0.1/wgpu/Cargo.toml).
