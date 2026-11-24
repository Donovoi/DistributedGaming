# Remote capture tool check

Date: 2026-11-24.

Goal progress: 20%. M3 and all later milestones remain pending.

## Host and build

The test ran on remote PC B, a physical Ubuntu 26.04.1 PC with an NVIDIA RTX 3070. The host driver stayed at 615.71.09. No driver was installed or replaced.

The tools ran as a non-root user inside an Ubuntu 24.04 container. Xorg used a separate display with private Xauthority, no TCP listener, no input-device attachment, and no VT switch. The container used the host name so its Xauthority entry matched. It did not use a privileged mode or an open X server.

| Item | Identity |
| --- | --- |
| Ubuntu base image | `sha256:562111b88caa79c71391c0c4d3c2114a7d5c35915c20561fd893cb1e07929e6b` |
| RenderDoc | 1.47 |
| RenderDoc source | `b0e1370309f7d7bf986a73830446ca39d9b8c1fd` |
| Built image | `sha256:288ac2044f74e9668e6a73de077703e86462b3418947ae32ecbde147e352bc04` |
| Dockerfile SHA-256 | `c0f3e3b1ae6c63d90ea5433be90daafa0ce27da18652586ee6e4c551fc4c78dd` |
| Smoke script SHA-256 | `a36bb6a10dd220e696dd7b5071596ad54e21cdaf895532e8deb00e1dcaa86b68` |

The base image and capture source are fixed. Apt packages remain time-dependent. The full package list is kept with the private run files. This image is not yet a fully repeatable release.

## Results

- The image built successfully on the remote PC.
- `renderdoccmd --version` and the Python module reported 1.47 as the intended non-root user.
- GLX reported direct rendering on the NVIDIA RTX 3070. Vulkan enumerated that discrete GPU and driver 615.71.09. No software renderer was accepted.
- The final smoke script captured one Vulkan draw from the 64-bit `vkcube` program.
- The capture opened and replayed on the remote GPU. Its 500 by 500 color output contained 1,000,000 bytes. Two replays gave equal bytes.
- The saved image was inspected. It showed the textured cube.
- Four regression checks passed in the remote image. They reject a uniform color with four distinct channel values, short data, and empty data, and accept distinct pixels.
- An existing output directory was rejected. Its original report hash stayed unchanged.

The [machine-readable result](capture-tools-2026-11-24.json) records hashes and the exact replay result. Captures and raw GPU reports remain private. No game files or account data are included.

## Resource handoff

The short-lived capture containers were removed after each check. The separate Xorg display remains for the authorized Steam setup. The two GPU services stopped for capture work were restored to their original active states. Their automatic restore timers were stopped after restoration was confirmed. No GPU workload remains paused for this capture check.

## Limits

This is a capture tool smoke test. It is not a game benchmark or a split rendering result. Repeating a captured draw proves repeat replay consistency; it does not compare that result with the original game's live image.

RenderDoc on Linux supports x86-64. The i386 runtime packages do not prove that a 32-bit game can be captured. The actual game binary must be checked first.

No frame-time or input-delay claim is made. Capture tests are separate from later performance runs.
