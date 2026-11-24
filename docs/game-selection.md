# First game selection

Date: 24 November 2026.

The user signed in to Steam. The visible library contained Half-Life 2 and Baldur's Gate 3. This record stores only the candidates needed for this project.

## Decision

Use Half-Life 2 for the first capture and replay test. Use an offline scene and a pinned game build. This is an engineering choice for repeatability and small scope. It is not evidence that the game will run faster across the network.

Half-Life 2 has a demo recording and playback system. Confirm the commands and graphics backend on the installed build. The Source SDK helps with concepts and interfaces; it is not a complete source release of the current commercial renderer. See [Valve's SDK](https://github.com/ValveSoftware/source-sdk-2013) and [Valve's demo documentation](https://developer.valvesoftware.com/wiki/Demo_Recording_Tools).

Keep Baldur's Gate 3 as a later Vulkan target. Larian lists Vulkan and DirectX 11 support. Confirm the actual backend and binary on the test PC before capture. See [Larian's requirements](https://larian.com/support/faqs/system-requirements_47).

## Pending checks

- Install the selected game on the coordinator through the user's Steam account.
- Record build ID, executable hash, graphics backend, resolution, and quality settings.
- Record and replay a fixed offline demo.
- Measure the single-PC result before any patch.
- Capture a frame and map its resource dependencies.

No game installation, capture, or patch is complete. M3 remains pending.

## Hardware constraint

Five Linux PCs were inspected through SSH. They have one RTX 4000 Ada, three RTX 3070 cards, and two GTX 1080 Ti cards in the fifth PC. Active wired interfaces reported 1 Gb/s. Virtual bridge speeds were excluded. Direct Tailscale probes used local-network endpoints. These checks do not measure sustained payload throughput.

At 1 Gb/s, a raw 1920 by 1080 RGBA8 image takes at least 66.36 ms to serialize. A worker must save enough compute to cover transfer and merge costs. Prefer small outputs, cached data, or expensive passes. GPU capability differs across these cards; the CPU harness does not test that difference.
