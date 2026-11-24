# Remote capture environment

Run this procedure on a remote Linux test PC. Do not run it on the user's desktop.

This environment captures a 64-bit Vulkan test program with RenderDoc. It does not run a game or prove a game speedup. M3 stays pending.

## Requirements

- One NVIDIA GPU, a working driver, Docker with the NVIDIA runtime, and Xorg.
- No active display session on the test PC. The example uses GPU 0 and display 97.
- Permission to run Docker and start the isolated X server.
- Free GPU memory. Record and restore any services stopped for a test. The user has approved temporary stops of the remote GPU workloads for this project.

The image requests host graphics libraries through the NVIDIA runtime. It does not install or replace the host driver. Missing host driver libraries remain a setup error.

## Build and start

1. From the repo on the remote PC, build the image:

   ```sh
   docker build -t distributed-gaming-capture tools/capture
   docker image inspect distributed-gaming-capture --format '{{.Id}}'
   ```

   The Ubuntu image and RenderDoc source commit are pinned. Apt package versions can change. Record the image ID and installed package versions for each experiment. This is not yet a fully repeatable release build.

2. Create a private work area outside the repo:

   ```sh
   work="$HOME/distributed-gaming"
   mkdir -p "$work/private/display" "$work/home" "$work/runtime" "$work/tools/capture"
   chmod 700 "$work/private" "$work/private/display" "$work/runtime"
   cp tools/capture/xorg.conf "$work/private/display/xorg.conf"
   cp tools/capture/smoke.py "$work/tools/capture/smoke.py"
   cp tools/capture/test_smoke.py "$work/tools/capture/test_smoke.py"
   test ! -e /tmp/.X11-unix/X97
   ```

   Stop if display 97 already exists. Do not replace another session.

3. Create an Xauthority file. Keep its value private:

   ```sh
   umask 077
   touch "$work/private/display/Xauthority"
   cookie=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
   xauth -f "$work/private/display/Xauthority" add :97 . "$cookie"
   unset cookie
   ```

4. Start the separate display. It has no TCP listener, takes no input devices, and stops after two hours:

   ```sh
   sudo systemd-run --unit=distributed-gaming-xorg-97 --property=RuntimeMaxSec=7200 \
     /usr/lib/xorg/Xorg :97 -config "$work/private/display/xorg.conf" \
     -auth "$work/private/display/Xauthority" -logfile "$work/private/display/Xorg.97.log" \
     -nolisten tcp -noreset -novtswitch
   ```

5. Open a container shell as your user. Use the host name so Xauthority matches. Do not use a login shell that changes HOME:

   ```sh
   docker run --rm -it --gpus device=0 --hostname "$(hostname)" \
     --user "$(id -u):$(id -g)" \
     -e HOME=/work/home -e DISPLAY=:97 \
     -e XAUTHORITY=/work/private/display/Xauthority -e XDG_RUNTIME_DIR=/work/runtime \
     -v /tmp/.X11-unix:/tmp/.X11-unix:ro -v "$work:/work" \
     distributed-gaming-capture bash
   ```

   No privileged container or open X server is needed for this capture check.

6. In that container, check the tools and renderer:

   ```sh
   renderdoccmd --version
   python3 -c 'import renderdoc as rd; print(rd.GetVersionString())'
   python3 /work/tools/capture/test_smoke.py
   glxinfo -B
   vulkaninfo --summary
   ```

   Require the intended NVIDIA GPU. Reject software rendering. `nvidia-smi` alone is not enough. Keep raw GPU reports private because they include hardware identifiers.

7. Capture and replay the test cube:

   ```sh
   timeout 90 python3 /work/tools/capture/smoke.py /work/private/capture-check-01
   ```

   Use a new output folder each time. Require a zero exit code, `repeat_replay_exact: true`, and `game_test: false`. Inspect `cube.png`; it must show the cube. The check requires Vulkan, NVIDIA capture metadata, draw events, a non-flat color output, and equal bytes from two replays. A renderer query must still confirm the actual replay GPU. The timeout bounds the whole check; the container removes any remaining test process when it exits.

8. Exit the container. Stop only the test display and restore any paused services:

   ```sh
   sudo systemctl stop distributed-gaming-xorg-97
   ```

   Confirm those services reach their original states. Remove the test Xauthority file when no test uses it. Keep captures and account files private.

## Limits before a game test

RenderDoc supports Linux x86-64. The i386 runtime packages and `compat32` setting do not provide 32-bit RenderDoc capture or missing 32-bit host NVIDIA libraries. Inspect the actual game's ELF class before choosing its capture path.

Steam is not installed in this image. Steam sign-in and game installation are separate work. The cube result does not close M3 or M4. M3 still needs a pinned game build, fixed settings, repeatable benchmark, and real game-frame capture. M4 needs verified replay of a selected game pass.

Keep timing runs separate from capture runs. Capture injection changes execution cost. No timing or input-delay claim comes from this smoke test.

Sources: [RenderDoc build requirements](https://github.com/baldurk/renderdoc/blob/v1.47/docs/CONTRIBUTING/Dependencies.md), [NVIDIA container graphics support](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/docker-specialized.html), and [NVIDIA display options for the tested driver](https://download.nvidia.com/XFree86/Linux-x86_64/615.71.09/README/xconfigoptions.html).
