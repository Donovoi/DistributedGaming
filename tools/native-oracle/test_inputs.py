"""Check failures before Vulkan setup. These tests do not execute a shader."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve())
env = dict(os.environ)
# If a case reaches Vulkan, it must fail for a different reason than input checks.
env["VK_DRIVER_FILES"] = "/missing-dg-native-oracle-icd.json"
with tempfile.TemporaryDirectory(prefix="dg-native-input-") as temp:
    root = Path(temp)
    shader, histogram, uniform = [root / n for n in ("shader", "histogram", "uniform")]
    shader.write_bytes(b"not the selected shader")
    histogram.write_bytes(bytes(1024))
    uniform.write_bytes(bytes(32))

    def rejected(message):
        result = subprocess.run(
            [binary, str(shader), str(histogram), str(uniform)],
            env=env, capture_output=True, text=True, timeout=10, check=False,
        )
        assert result.returncode != 0, "Invalid input was accepted"
        assert message in result.stderr, result.stderr
        assert result.stdout == "", "Failed input produced a success report"

    rejected("Shader identity mismatch")
    for path, size in ((histogram, 1024), (uniform, 32)):
        for length in (size - 1, size + 1):
            path.write_bytes(bytes(length))
            rejected("Wrong input size")
        path.write_bytes(bytes(size))
    shader.write_bytes(bytes(1024 * 1024 + 1))
    rejected("Wrong input size")
    shader.unlink()
    rejected("Missing input")
print("Native oracle: seven invalid inputs rejected before GPU setup")
