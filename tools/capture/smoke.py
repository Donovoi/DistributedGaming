"""Check a 64-bit Vulkan capture on a remote GPU. This is not a game test."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import time

import renderdoc as rd


def require_success(result, operation):
    if result != rd.ResultCode.Succeeded:
        raise RuntimeError(f"{operation} failed: {result}")


def actions_in(roots):
    for action in roots:
        yield action
        yield from actions_in(action.children)


def require_varied_pixels(data, width, height):
    if len(data) != width * height * 4 or not data:
        raise RuntimeError("The captured color data has the wrong length.")
    if all(data[i:i + 4] == data[:4] for i in range(0, len(data), 4)):
        raise RuntimeError("The captured output is flat.")


def run(out):
    out.mkdir(parents=True, exist_ok=False)
    app = Path("/usr/bin/vkcube")
    if app.read_bytes()[:5] != b"\x7fELF\x02":
        raise RuntimeError("The capture test requires a 64-bit ELF vkcube.")
    rd.InitialiseReplay(rd.GlobalEnvironment(), [])
    target = None
    pid = None
    capture = None
    controller = None
    try:
        launched = rd.ExecuteAndInject(
            str(app), str(out), "--c 10000", [], str(out / "cube"),
            rd.CaptureOptions(), False,
        )
        require_success(launched.result, "Launch")
        target = rd.CreateTargetControl("", launched.ident, "dg-capture-check", False)
        if target is None:
            raise RuntimeError("Cannot connect to the capture target.")
        pid = target.GetPID()
        target.TriggerCapture(1)
        deadline = time.monotonic() + 30
        capture_path = None
        while time.monotonic() < deadline:
            message = target.ReceiveMessage(None)
            if message.type == rd.TargetControlMessageType.NewCapture:
                capture_path = Path(message.newCapture.path)
                break
            if message.type == rd.TargetControlMessageType.Disconnected:
                raise RuntimeError("The capture target disconnected.")
        if capture_path is None:
            raise RuntimeError("No capture arrived within 30 seconds.")
        target.Shutdown()
        target = None
        os.kill(pid, signal.SIGTERM)
        pid = None

        capture = rd.OpenCaptureFile()
        require_success(capture.OpenFile(str(capture_path), "", None), "Open capture file")
        status, controller = capture.OpenCapture(rd.ReplayOptions(), None)
        require_success(status, "Open replay")
        api = controller.GetAPIProperties()
        if api.pipelineType != rd.GraphicsAPI.Vulkan:
            raise RuntimeError("The capture is not Vulkan.")
        if api.vendor != rd.GPUVendor.nVidia or api.degraded:
            raise RuntimeError("The smoke test requires a full NVIDIA replay.")
        draws = [a for a in actions_in(controller.GetRootActions())
                 if a.flags & rd.ActionFlags.Drawcall]
        if not draws:
            raise RuntimeError("The capture has no draw events.")
        last = draws[-1]
        output = last.outputs[0]
        if output == rd.ResourceId.Null():
            raise RuntimeError("The last draw has no color output.")
        texture = next(t for t in controller.GetTextures() if t.resourceId == output)
        if (texture.format.compCount != 4 or texture.format.compByteWidth != 1
                or texture.depth != 1 or texture.arraysize != 1):
            raise RuntimeError("The smoke test requires a four-byte color pixel layout.")
        controller.SetFrameEvent(last.eventId, True)
        first = bytes(controller.GetTextureData(output, rd.Subresource()))
        require_varied_pixels(first, texture.width, texture.height)
        controller.SetFrameEvent(0, True)
        controller.SetFrameEvent(last.eventId, True)
        second = bytes(controller.GetTextureData(output, rd.Subresource()))
        if first != second:
            raise RuntimeError("Repeated replay changed the output bytes.")
        save = rd.TextureSave()
        save.resourceId = output
        save.destType = rd.FileType.PNG
        require_success(controller.SaveTexture(save, str(out / "cube.png")), "Save image")
        report = {
            "test": "vkcube capture and replay; not a game",
            "renderdoc_version": rd.GetVersionString(),
            "api": "Vulkan",
            "capture_vendor": "NVIDIA",
            "replay_degraded": api.degraded,
            "draw_count": len(draws),
            "output_width": texture.width,
            "output_height": texture.height,
            "output_bytes": len(first),
            "output_sha256": hashlib.sha256(first).hexdigest(),
            "capture_sha256": hashlib.sha256(capture_path.read_bytes()).hexdigest(),
            "repeat_replay_exact": True,
            "game_test": False,
        }
        (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))
    finally:
        if controller is not None:
            controller.Shutdown()
        if capture is not None:
            capture.Shutdown()
        if target is not None:
            target.Shutdown()
        if pid is not None:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        rd.ShutdownReplay()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path, help="New private output directory")
    run(parser.parse_args().out.resolve())
