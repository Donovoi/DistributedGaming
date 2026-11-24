# Installed game compatibility check

Date: 2026-11-24. Location: remote PC A, Linux. Repository base: `016688e3fdbd23890783f2058002d26742f1507e`.

## Method

Install each game through the signed-in official Steam client. Wait for its local app manifest to report installed state (`StateFlags` = 4). Read only the needed build fields. Read the ELF headers and hash the selected binaries on the remote PC. Keep account data and game files private.

The final inspection used these commands. Start in the Steam `steamapps` folder on the remote test PC. The build IDs come from the local app manifests, not from executable build tags.

```sh
sed -n -E '/"(appid|buildid|StateFlags)"[[:space:]]/p' appmanifest_220.acf appmanifest_203160.acf
cd 'common/Half-Life 2'
file hl2_linux bin/engine.so bin/launcher.so bin/libdxvk_d3d9.so bin/shaderapivk.so
sha256sum hl2_linux bin/engine.so bin/launcher.so bin/libdxvk_d3d9.so bin/shaderapivk.so
cd '../Tomb Raider'
file bin/TombRaider
sha256sum bin/TombRaider
```

An ELF class value of 1 means 32-bit. Machine value 3 means x86. The installed files below have both values. The capture tool is RenderDoc 1.47, whose Linux support is x86-64. See [RenderDoc's supported platforms](https://github.com/baldurk/renderdoc/blob/v1.47/docs/CONTRIBUTING/Compiling.md).

## Result

| Game | Steam app | Build | Inspected code | Result |
| --- | --- | --- | --- | --- |
| Half-Life 2 | 220 | 19307283 | Launcher, engine, Vulkan wrapper | ELF32 x86. Reject current capture route. |
| Tomb Raider | 203160 | 9573671 | `bin/TombRaider` | ELF32 x86. Reject current capture route. |

See [Half-Life 2 hashes](hl2-build-19307283.json) and [Tomb Raider hash](tomb-raider-build-9573671.json). The first record uses numeric ELF header values. The second uses their text names. Both describe the same architecture.

These are local installation and file identity checks. The selected hashes do not prove complete Steam file integrity. The records cover the installed Linux builds only. They do not cover Windows builds, other branches, or other capture tools.

Neither game was launched, benchmarked, or captured. No game milestone closes. Progress remains 20%.

## Decision

Check Counter-Strike 2 next. Verify the actual installed binary, Vulkan device, and local insecure mode. A supported architecture alone does not prove capture will work. Keep its baseline run separate from its frame capture.
