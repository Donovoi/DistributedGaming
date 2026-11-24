# Vulkan observer

Identify one fixed shader and record its direct dispatches and `vkQueueSubmit` calls. This tool observes one process. It does not replace game work, read GPU buffers, add GPU commands, or send network jobs.

Use this experimental layer on a remote Linux x86-64 PC. It uses the Vulkan loader interface version 2. The only exported entry point is the negotiation function. Private function pointers avoid collisions with the loader's own ELF symbols.

## Build and check

Run these commands on a remote PC or a suitable VM. Use Rust 1.93.1, a C compiler, and Vulkan development headers and loader.

```sh
cargo +1.93.1 fmt --manifest-path tools/vulkan-observer/Cargo.toml --check
cargo +1.93.1 clippy --manifest-path tools/vulkan-observer/Cargo.toml --all-targets --locked -- -D warnings
cargo +1.93.1 test --manifest-path tools/vulkan-observer/Cargo.toml --all-targets --locked
cargo +1.93.1 build --manifest-path tools/vulkan-observer/Cargo.toml --release --locked
cc -std=c11 -Wall -Wextra -Werror tools/vulkan-observer/check_abi.c -o /tmp/check-abi
/tmp/check-abi
cc -std=c11 -Wall -Wextra -Werror tools/vulkan-observer/check_loader.c -lvulkan -ldl -o /tmp/check-loader
/tmp/check-loader "$PWD/tools/vulkan-observer/target/release/libdg_vulkan_observer.so"
```

Expect 15 Rust tests and both C checks to pass. The C checks compare the loader layout with the installed header and verify that negotiated function pointers belong to this library. They do not use a GPU. CI runs these checks on a Linux VM.

## Check the original shader

Use the [native oracle](../native-oracle/README.md), its private shader, and one fixed input pair. Stop and restore conflicting GPU work under a separate restore timer. Run without Python optimization flags:

```sh
python3 tools/vulkan-observer/check_oracle.py \
  NATIVE_ORACLE PRIVATE_SHADER PRIVATE_HISTOGRAM PRIVATE_UNIFORM NEW_PRIVATE_RUN_DIR
```

The script runs five modes: baseline, observer, missing log directory, existing log file, and denied file writes. Each process has a 45-second timeout. Each mode executes the oracle's three output sentinel cases. It requires identical oracle JSON, zero core/synchronization validation errors, unchanged inputs and guards, and three direct dispatch/submission records in the normal observer log. The existing file must remain unchanged. The write-denied log must be empty.

The [recorded check](../../docs/validation/vulkan-observer-2026-11-24.md) passed on two captured input pairs. No live game run has been checked with this layer yet. A host-built library can require a newer glibc than the Steam runtime; build and check inside that runtime before a game trial.

## Enable for one process

Use a new absolute log path. Keep raw logs private. Do not install the manifest as a global or implicit layer.

```sh
env VK_ADD_LAYER_PATH="$PWD/tools/vulkan-observer" \
  VK_INSTANCE_LAYERS=VK_LAYER_DG_observer \
  DG_OBSERVER_LOG=/ABSOLUTE/PRIVATE/NEW_LOG.jsonl \
  timeout 45 TEST_PROGRAM
```

`VK_ADD_LAYER_PATH` keeps the normal layer search paths available. The native oracle still requires the Khronos validation layer. The observer uses `create_new` for its log; it never overwrites an existing file. An unavailable log does not block forwarding. A regular-file write failure ends logging. Missing, truncated, or failed logs are incomplete evidence.

## Read the records

- `target_shader` identifies the compiled SHA-256 in `src/track.rs`. Other shader bytes are not saved.
- `target_pipeline` assigns an ID to a known pipeline lifetime. Its identity survives shader-module destruction. Repeated live shader/pipeline handle values use reference counts. Conflicting identities become unknown.
- `target_dispatch_recorded` records a direct dispatch against a known bound pipeline. Command IDs and recording epochs separate allocation and reset lifetimes.
- `target_submission` records known command buffers passed to `vkQueueSubmit`. `success` means the API returned success. It does not prove completion. Partial command lists set `command_list_complete` to false and `unknown_commands` to true.
- `instance_lifetime_end` reports open instances, observed counts, dropped records, failures, and unknown observations. Extension function queries have a separate count. A query can come from the loader; it does not prove that the function was called.

Require a normal process exit and a final lifetime record with zero open instances before using the log. Missing tails or failure flags invalidate completeness claims. Even a clean tail proves only the listed observations. It is not proof that every possible GPU work path was seen.

## Limits

Descriptor contents, update templates, buffer views, memory bindings, aliases, mapping lifetimes, completion, and the eventual output consumer are not tracked. Resource correctness and execution flags remain false. The layer cannot authorize a replacement patch.

Secondary commands, indirect dispatch, dispatch-base calls, shader-object binds, and Submit2 calls are forwarded but marked unknown. Shader-object binds clear the remembered pipeline identity. Core and KHR aliases call the matching downstream function. Device-generated commands and other unlisted extensions are outside coverage. Unknown calls and missing target records must never be read as proof that the game did no target work.

The layer bounds observed arrays at 4,096 entries, shader hashing at one MiB, and retained objects at 65,536. Regular logs allow at most 4,096 records and eight MiB. All lifetime footers share a 16-KiB reserve, with a small terminal marker if that reserve is exhausted. Loader-chain search is limited to 256 nodes. These are diagnostic limits, not changes to the arrays sent downstream.

Mandatory dispatch lookup is separate from observation state. No state or log lock is held across a downstream Vulkan call. Destruction retires records before forwarding, so later handle reuse cannot be erased by old cleanup. Observation panics are caught and flagged. Invalid Vulkan pointers, missing mandatory dispatch state, allocator failure, and driver faults are not recoverable guarantees.

Hashing, locks, and synchronous file writes add CPU work and can delay the calling thread. Do not use observer runs as performance results. No timing or speedup claim is made here.

Sources: [Khronos loader interface](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderLayerInterface.md), [loader ABI header](https://github.com/KhronosGroup/Vulkan-Headers/blob/main/include/vulkan/vk_layer.h), and [Vulkan object model](https://github.com/KhronosGroup/Vulkan-Docs/blob/main/chapters/fundamentals.adoc).
