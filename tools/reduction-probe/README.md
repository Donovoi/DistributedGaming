# Fixed game-pass experiment

This Rust tool checks one reimplementation of a captured Counter-Strike 2 compute pass. It uses `wgpu` and an embedded WGSL shader. It does not execute the original shader binary, patch a running game, or distribute work.

Only one private input case is accepted. Exact byte sizes and SHA-256 hashes are checked before GPU setup. The shader, bindings, and one-workgroup dispatch are fixed in the source. The tool loads no shader code or commands from the input directory. Its own Rust code forbids unsafe code.

## Run the check

Run all commands on a remote Linux test PC. The tested GPU is an RTX 3070 using Vulkan. CI builds on Linux and Windows, but CI does not run this GPU experiment.

1. Build and check the tool from the repo root:

   ```sh
   cargo +1.93.1 fmt --manifest-path tools/reduction-probe/Cargo.toml --check
   cargo +1.93.1 clippy --manifest-path tools/reduction-probe/Cargo.toml --all-targets --locked -- -D warnings
   cargo +1.93.1 test --manifest-path tools/reduction-probe/Cargo.toml --all-targets --locked
   cargo +1.93.1 build --manifest-path tools/reduction-probe/Cargo.toml --release --locked
   ```

2. Use the private reference directory from the recorded capture. It must contain `session-0-histogram.bin` (1,024 bytes), `session-0-uniform.bin` (32 bytes), and `session-0-output.bin` (4 bytes). The hashes must match the source and [test record](../../docs/validation/cs2-reduction-2026-11-24.json). These game resource snapshots are not distributed with the repo. Missing data is an error; the tool does not generate a replacement case.
3. Record and stop the competing GPU service under a restore timer. Run the tool with an outer process limit:

   ```sh
   timeout 45 tools/reduction-probe/target/release/dg-reduction-probe PRIVATE_REFERENCE_DIRECTORY
   ```

   Set `XDG_RUNTIME_DIR` to a private directory owned by the test user if the host needs it. The tool uses no display window. It selects the first NVIDIA discrete Vulkan adapter. Do not assume which card it selects on a PC with several GPUs; check the reported adapter. Adapter setup is outside the tool's five-second readback wait. The outer timeout bounds the whole process.

4. Require exit code zero. All three run records must have `exact`, `guards_unchanged`, and `inputs_unchanged` set to true. The native output bit pattern for this case is 1041600569. `original_shader_executed` and `game_speedup_proven` must remain false. Restore the GPU service and check its state.

The three CPU tests check missing files, wrong sizes, and the difference between size and identity. They do not check GPU arithmetic. The private GPU run is recorded separately.

## Limits

The native reference comes from the original shader during unmodified capture replay. It is not a readback of a live game's original output. The separate whole-frame replay mismatch remains unresolved.

The first capture's inputs matched at the two selected event endpoints. RenderDoc replays the prefix for each event selection, so those snapshots do not prove that native inputs stayed unchanged within one execution. The Rust probe's own input and guard checks apply to its isolated dispatch and are separate checks.

The WGSL calculation matched this one native reference on the tested GPU. A scalar implementation differed by one floating-point step and failed the exact check. No tolerance was added. Exact results may differ on other hardware or for other inputs.

M4 still needs a second, different captured case, arithmetic edge cases, and a complete execution contract. This tool checks fixed inputs; it is not a general game profile loader. It supplies no evidence of LAN benefit. See the [experiment record](../../docs/validation/cs2-reduction-2026-11-24.md).
