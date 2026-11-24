# First game selection

Date: 24 November 2026.

The user signed in to the official Steam client on a remote PC. This record stores only candidates needed for this project.

## Decision

Use Counter-Strike 2 for the current tests. Build 26879581 has an ELF64 x86-64 executable. A local SourceTV demo was captured through Vulkan on a remote RTX 3070. Replay shows world geometry, but repeated output did not match exactly. See the [capture evidence](validation/cs2-capture-2026-11-24.md). Use only a local scene with `-insecure`. Verify that mode before instrumentation. Do not enter matchmaking during these tests. See [Valve's requirements](https://store.steampowered.com/app/730/CounterStrike_2/).

The first choice was Half-Life 2. Direct inspection found ELF32 x86 in its installed launcher, engine, and Vulkan wrapper. Tomb Raider's installed Linux executable is also ELF32 x86. Linux RenderDoc supports x86-64. Reject these builds for this capture route. This is not a finding that distributed work is impossible in either game. See the [build evidence](validation/game-builds-2026-11-24.md) and [RenderDoc platform support](https://github.com/baldurk/renderdoc/blob/v1.47/docs/CONTRIBUTING/Compiling.md).

Keep Baldur's Gate 3 as a later candidate. Its native Linux release targets Steam Deck. Larian does not support other Linux PCs. Do not assume this build works on the test NVIDIA PCs. See [Larian's native build FAQ](https://larian.com/support/faqs/steam-deck-native-version_121).

## Pending checks

- Complete a repeatable single-PC benchmark without RenderDoc. Pin settings and actual output size before and after the measured runs. Record the loaded measurement tools.
- Keep the SourceTV build, map, demo hash, camera, mode, tick interval, and playback speed fixed. A spectator result is not a player input test.
- Quantify and explain the replay mismatch. Verify a selected pass and map all its resource dependencies.

Half-Life 2 and Tomb Raider are installed. Neither was run or captured. Counter-Strike 2 has a real frame capture, but no accepted baseline or verified patch. M3 and M4 remain pending.

## Hardware constraint

Five Linux PCs were inspected through SSH. They have one RTX 4000 Ada, three RTX 3070 cards, and two GTX 1080 Ti cards in the fifth PC. Active wired interfaces reported 1 Gb/s. Virtual bridge speeds were excluded. Direct Tailscale probes used local-network endpoints. These checks do not measure sustained payload throughput.

At 1 Gb/s, a raw 1920 by 1080 RGBA8 image takes at least 66.36 ms to serialize. A worker must save enough compute to cover transfer and merge costs. Prefer small outputs, cached data, or expensive passes. GPU capability differs across these cards; the CPU harness does not test that difference.
