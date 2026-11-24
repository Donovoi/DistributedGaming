# Fixed game-pass experiment

This Rust tool checks one reimplementation of a captured Counter-Strike 2 compute pass. It uses `wgpu` and an embedded WGSL shader. It does not execute the original shader binary, patch a running game, or distribute work.

Only the 11 cases in the compiled [case list](cases.json) are accepted. They comprise two private captured inputs and nine synthetic arithmetic inputs. Each complete tuple of histogram, uniform, and reference-output hashes must match one case. Exact sizes and identities are checked before GPU setup. The shader, bindings, and one-workgroup dispatch are fixed. The tool loads no shader code or commands from the input directory. Its own Rust code forbids unsafe code.

## Run the check

Run all commands on a remote Linux test PC. The tested GPU is an RTX 3070 using Vulkan. CI builds on Linux and Windows, but CI does not run this GPU experiment.

1. Build and check the tool from the repo root:

   ```sh
   cargo +1.93.1 fmt --manifest-path tools/reduction-probe/Cargo.toml --check
   cargo +1.93.1 clippy --manifest-path tools/reduction-probe/Cargo.toml --all-targets --locked -- -D warnings
   cargo +1.93.1 test --manifest-path tools/reduction-probe/Cargo.toml --all-targets --locked
   cargo +1.93.1 build --manifest-path tools/reduction-probe/Cargo.toml --release --locked
   ```

2. Use a fixed reference directory. It must contain `session-0-histogram.bin` (1,024 bytes), `session-0-uniform.bin` (32 bytes), and `session-0-output.bin` (4 bytes). The hashes must match one compiled case. The captured inputs remain private. To write the nine public arithmetic fixtures, run `python3 tools/reduction-probe/make_synthetic.py runs/reduction-synthetic`. This copies saved oracle output bits; it does not run the original shader. To reproduce those bits independently, run the [native oracle](../native-oracle/README.md) with the pinned private shader and each generated input pair.
3. Record and stop the competing GPU service under a restore timer. Run the tool with an outer process limit:

   ```sh
   timeout 45 tools/reduction-probe/target/release/dg-reduction-probe PRIVATE_REFERENCE_DIRECTORY
   ```

   If needed, set `XDG_RUNTIME_DIR` for the probe process only. Do not change it for the shell that restores user services. The tool uses no display window. It selects the first NVIDIA discrete Vulkan adapter. Do not assume which card it selects on a PC with several GPUs; check the reported adapter. Adapter setup is outside the tool's five-second readback wait. The outer timeout bounds the whole process.

4. Require exit code zero. All three run records must have `exact`, `guards_unchanged`, and `inputs_unchanged` set to true. Output bits must match the selected case. `original_shader_executed` and `game_speedup_proven` must remain false. Restore the GPU service and check its state.

The six CPU tests check missing files, wrong sizes, altered input and output files, mixed-case hashes, and consistency of the compiled reference data. They do not check GPU arithmetic. CI also verifies the generated synthetic file identities. The GPU results are recorded separately.

## Limits

The reference outputs now come from the original shader running with fixed isolated inputs. The shader hash is pinned by the native oracle. Those outputs also match the two sampled game references. This is not a readback of a live game's original output. The separate whole-frame replay mismatch remains unresolved.

The first capture's inputs matched at the two selected event endpoints. RenderDoc replays the prefix for each event selection, so those snapshots do not prove that native inputs stayed unchanged within one execution. The Rust probe's own input and guard checks apply to its isolated dispatch and are separate checks.

The unchanged WGSL calculation matched the original shader on all 11 recorded cases. A scalar implementation differed by one floating-point step on the first case and failed the exact check. No tolerance was added. Exact results may differ on other hardware or for other inputs. NaNs, subnormals, and floating-point overflow are outside this suite.

M4 covers this isolated pass replay. Integration into the running game still requires its original resource ownership, alias, and producer/consumer synchronization checks. This tool is not a general game profile loader. It supplies no evidence of LAN benefit. See the [suite record](../../docs/validation/cs2-replay-suite-2026-11-24.md).
