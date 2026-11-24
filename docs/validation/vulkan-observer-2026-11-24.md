# Vulkan observer check

Date: 2026-11-24 UTC. Location: remote PC A. Repository base: `7acd250ff1008f0722b8cd705efe09a75cc5ced7`. Goal progress stays at 50%. M5 is pending.

## Question and method

Can a process-scoped Rust Vulkan layer identify the fixed CS2 shader, retain its pipeline identity across object lifetimes, and record direct dispatch/submission without changing the native oracle's result?

Use the original shader and the two fixed captured inputs from the [M4 suite](cs2-replay-suite-2026-11-24.md). Compare the complete native-oracle JSON with the layer disabled and in four enabled modes. Keep core and synchronization validation enabled. Use three output sentinels in every process.

## Result

| Case | Baseline output bits | All four enabled modes | Native executions |
| --- | ---: | --- | ---: |
| First captured input | 1041600569 | Exact match | 15 |
| Second captured input | 1034300058 | Exact match | 15 |

All 30 executions returned the expected bits. Inputs and guard bytes stayed unchanged. Core and synchronization validation reported zero errors through teardown. The enabled modes used a normal log, a missing log directory, an existing log, and a zero file-size limit that forced write failure. Existing log bytes stayed unchanged. The write-failure log stayed empty. All modes returned the same oracle JSON as their baseline.

Each normal log contained three target dispatch records and three successful submissions. Command lists were complete. No unknown observations, dropped records, I/O failures, or observation panics were reported. The final record had zero open instances. One extension function query was reported in each process; this was not counted as an executed unsupported call.

The final remote build passed Rust format, clippy with warnings denied, all 15 tests, release build, and the C ABI check. Tests cover failed calls, partial pipeline creation, two-device routing, observer panic, handle reuse during destruction, repeated live handles, command reset/end failures, mixed known/unknown work, partial submissions, extension aliases, stale shader-object bindings, and log exhaustion/failure. The separate loader check is required in CI and checks negotiated pointer ownership with libvulkan already loaded.

The GPU service was active before each GPU test batch. A bounded restore timer and exit trap protected the test. The service was confirmed active afterward. No game or local desktop test was run.

## Faults found and fixed

The first GPU attempt stopped during loader setup. Exported Vulkan names resolved to loader symbols on Linux. The layer now exports only its negotiation entry point and returns private function pointers. The second attempt loaded and passed the original-shader check. The final suite above used the reviewed lifetime, completeness, and extension fixes.

The independent review found cleanup races, duplicate live handles, false completeness on truncated submissions, missing command taint, and stale pipeline identity after shader-object binds. These were fixed and covered by regression tests. See [review 0012](../reviews/0012.md).

## Decision and limits

Use this layer as the starting point for resource observation. This check establishes forwarding and output preservation for the two isolated cases on the tested RTX 3070 and driver 615.71.09. It does not establish live game compatibility, full Vulkan coverage, game resource ownership, whole-frame equality, input delay, or distributed benefit.

The shader observer does not know descriptor contents or the output consumer. Next add descriptor-template and resource lifetime tracking. Check that work on the oracle, then use an isolated offline game run built for the Steam runtime. Keep the proposed replacement disabled until its resource contract is complete.

Exact source and binary identities and the checked mode summaries are in the [machine record](vulkan-observer-2026-11-24.json). The native oracle binary hash is `4ad85a66dce75a0dc581362959586f146efec4012f6744cb183de43444939f47`. CI must pass for the final PR head before merge.
