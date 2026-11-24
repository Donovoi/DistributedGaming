# Native shader oracle

Run one fixed, original CS2 shader with supplied input bytes. This is a validation tool. It does not load a game profile, patch a game, or send work over a network.

The product harness remains Rust. This small C++ tool uses Vulkan storage texel buffers, which the safe wgpu binding API does not expose. It executes the original SPIR-V bytes. The separate Rust probe executes a WGSL reconstruction.

## Build and run

Use a remote Linux PC. Install a C++17 compiler, Vulkan headers and loader, OpenSSL headers, and the Khronos validation layer. Keep the game shader and input files private.

```sh
g++ -std=c++17 -O2 -Wall -Wextra -Werror -Wno-missing-field-initializers \
  tools/native-oracle/main.cpp -lvulkan -lcrypto -o /tmp/native-oracle
python3 tools/native-oracle/test_inputs.py /tmp/native-oracle
timeout 45 /tmp/native-oracle PRIVATE_SHADER PRIVATE_HISTOGRAM PRIVATE_UNIFORM
```

The histogram must contain 1,024 bytes. The uniform range must contain 32 bytes. The shader must match the SHA-256 in the source. The tool checks these conditions before Vulkan setup. Shader reads are limited to one MiB. CI builds the tool and checks seven invalid inputs. CI does not run the private GPU cases.

## Fixed execution

Use a discrete NVIDIA GPU with Vulkan 1.2 or later. The tool requires core and synchronization validation. It checks device limits and storage texel format support. It uses entry point `MainCs`, one workgroup of 256 threads, no specialization values, and no push constants.

| Set | Binding | Type | Format | Bytes |
| --- | --- | --- | --- | --- |
| 0 | 159 | Storage texel buffer | R32_UINT | 1,024 |
| 0 | 2 | Uniform buffer | None | 32 |
| 0 | 160 | Storage texel buffer | R32_SFLOAT | 4 |

Each buffer has a separate allocation. The output view starts at a valid device alignment. Guard bytes surround its four-byte range. Input buffers also have tail guards. The tool checks host-visible coherent memory, explicit host-to-compute and compute-to-host barriers, and fence completion before readback.

Each case runs with three output starting values. Success requires the same output bits each time, unchanged input bytes and guards, and zero validation errors through device teardown. The JSON reports the actual output. It does not compare that output with a captured frame; the experiment record performs that separate comparison.

The fence wait is five seconds. If submitted work fails to finish, the process exits without waiting for device cleanup. Always keep the external 45-second timeout: Vulkan initialization and driver calls do not have per-call deadlines.

## Limits

The shader hash fixes the shader interface. The tool does not verify a caller-supplied descriptor contract. It accepts only its compiled interface. It does not prove that the game's resources lacked aliases or that its original synchronization was correct.

The [replay suite](../../docs/validation/cs2-replay-suite-2026-11-24.md) checks two captured inputs and nine bounded arithmetic cases against the unchanged Rust/WGSL reconstruction. All recorded outputs are finite and normal. Other input domains, other drivers, and completion-failure injection remain unverified. A validation pass does not prove whole-frame correctness or LAN benefit. The [first native experiment](../../docs/validation/cs2-native-oracle-2026-11-24.md) remains a separate historical record.
