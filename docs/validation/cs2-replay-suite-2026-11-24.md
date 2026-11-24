# Selected pass replay suite

Date: 2026-11-24. Location: remote PC A. M4 complete. Goal progress: 50%.

The unchanged captured shader and unchanged WGSL reconstruction produced identical output for two captured-input fixtures and nine arithmetic cases. Each case used three output starting values. All 33 comparisons matched every output bit. All input bytes and guards stayed unchanged. The original-shader runs passed core and synchronization validation.

This completes M4 for one isolated pass, on the recorded RTX 3070 and driver 615.71.09, for the 11 compiled cases. Original game resource aliasing, producer/consumer ordering, and safe replacement remain unproved. The full-frame replay mismatch remains open. No game patch, distributed pass, or speedup is accepted.

The [machine record](cs2-replay-suite-2026-11-24.json) gives source and binary hashes, input identities, definitions, output bits, and limits. The [case list](../../tools/reduction-probe/cases.json) is compiled into the Rust probe. Game assets remain private. The [fixture generator](../../tools/reduction-probe/make_synthetic.py) writes only synthetic inputs and saved reference output bits; it does not execute the original shader.

## Method

Hypothesis: the reconstruction preserves the selected shader's result for a different captured input and for bounded integer edge cases.

1. Freeze two captured input sets and nine arithmetic definitions. Generate native reference outputs with the pinned original shader. Keep the input files and native results unchanged during comparison.
2. Keep the WGSL hash unchanged from the first experiment: `a5e66faefef644f61e4272c65ea6fcc250a82085f5881e375029ae44243d0e7c`. Change only case loading, result labels, and input tests in the Rust program.
3. Require one complete tuple of histogram, uniform, and native-output hashes from the compiled list. Reject unknown or mixed tuples before GPU setup.
4. Run each case with output starting values 0, 4294967295, and 305419896. Compare all four output bytes with the independent native result. Check all 1,056 input bytes and the output guards. No tolerance is allowed.
5. Retain the two implementations' results separately. Require exit code zero, exact output, unchanged inputs and guards, and no native validation errors.

The second fixture uses its new standalone oracle output. The earlier rejected endpoint comparison is not its paired reference. The saved first fixture also stays fixed: later full-capture replays produced different input bytes.

## Arithmetic cases

Unlisted bins are zero. Every uniform byte is initialized; the unused final 16 bytes are zero. Wrapping cases test arithmetic rules and are not claimed to be plausible game histograms.

| Case | Nonzero bins | Width × height | Minimum, extent | Wrapped weighted sum |
| --- | --- | --- | --- | ---: |
| Zero | None | 1280 × 720 | -8, 13 | 0 |
| Bin 0 | h[0] = 921600 | 1280 × 720 | -8, 13 | 0 |
| Bin 255 | h[255] = 921600 | 1280 × 720 | -8, 13 | 235008000 |
| Product wrap | h[255] = 0x01010102 | 1 × 1 | 0, 1 | 254 |
| Sum wrap | h[1] = 0xffffffff; h[2] = 1 | 1 × 1 | 0, 1 | 1 |
| Zero width | h[255] = 1 | 0 × 720 | 0, 1 | 255 |
| Zero height | h[255] = 1 | 1280 × 0 | 0, 1 | 255 |
| Dimension wrap | h[255] = 65536 | 65536 × 65537 | 0, 1 | 16711680 |
| Dimension wrap to zero | h[255] = 1 | 65536 × 65536 | 0, 1 | 255 |

The dimension product wraps as a 32-bit unsigned integer, then the shader clamps the floating-point denominator to at least one. All recorded outputs are finite and normal. NaNs, subnormals, floating-point overflow, and other input domains are outside this suite. Exact output bits come from the original shader; the integer table alone is not a floating-point oracle.

## Local execution contract

The pinned original SPIR-V declares only Shader and ImageBuffer capabilities. Its entry point is `MainCs`; local size is 256 × 1 × 1. The dispatch is 1 × 1 × 1. Its reflected state has no specialization values, push constants, or required subgroup size.

Set 0 binding 159 reads a 1,024-byte R32_UINT texel-buffer view. Binding 2 reads the dimensions and range from the first 16 bytes of a supplied 32-byte uniform range. Binding 160 writes one R32_SFLOAT value through a four-byte texel-buffer view. The workgroup uses 1,024 bytes of shared scratch. The inspected instructions contain no other resource access. The native oracle preserves these types and ranges; the reconstruction maps the same supplied bytes to fixed WGSL storage and uniform bindings.

Each oracle buffer has a separate allocation. Its output view is aligned and has guard bytes outside the view. Host initialization, compute execution, readback barriers, and fence completion are explicit. The shader has no remaining unknown local input for this isolated execution. These facts do not establish the game's original memory aliasing or surrounding barriers. Those checks are required before M5 integration.

## Validation and remaining work

Formatting, Clippy with warnings denied, six CPU tests, and the release build passed remotely. The tests reject missing files, wrong sizes, changed inputs and references, and mixed tuples. The native oracle's seven rejection checks remain in CI. The public generator reproduced all nine synthetic file identities. CI builds on Linux and Windows; it does not run the private GPU suite.

The final remote source was in `tools/reduction-probe-suite`. Its build reused `tools/reduction-probe/target` through `CARGO_TARGET_DIR`. The old source next to that target was not the source of this binary. The machine record names both directories and pins the final source and saved executable. The repository's updated `tools/reduction-probe/src/main.rs` has that final source hash.

An earlier wrapper changed `XDG_RUNTIME_DIR` for its whole shell, so its service-restore command failed. A fresh session restored the GPU service immediately. The final wrapper changes that value only for the probe process. Its restore check passed. Both attempts remain in private logs.

The scalar reconstruction's earlier one-step failure remains recorded. No tolerance was added to hide it. There is no general proof for all arithmetic inputs, hardware, drivers, or game builds. The tiny selected reduction has no demonstrated LAN benefit. Next: design and verify a resource-safe two-PC pass path, then test live-game integration and failure handling before measuring benefit.

Sources: [captured native contract and validation](cs2-native-oracle-2026-11-24.md), [SPIR-V instruction rules](https://registry.khronos.org/SPIR-V/specs/unified1/SPIRV.html), and [WGSL floating-point limits](https://www.w3.org/TR/WGSL/#floating-point-evaluation).
