"""Write nine fixed arithmetic fixtures and their saved oracle outputs.

This copies expected bits from the compiled case record. It does not execute
the original shader. Use native-oracle to reproduce those outputs independently.
"""
import hashlib
import json
from pathlib import Path
import struct
import sys

CASES = [
    ("zero", [], [1280, 720, -8.0, 13.0]),
    ("bin-0", [(0, 921600)], [1280, 720, -8.0, 13.0]),
    ("bin-255", [(255, 921600)], [1280, 720, -8.0, 13.0]),
    ("product-wrap", [(255, 0x01010102)], [1, 1, 0.0, 1.0]),
    ("sum-wrap", [(1, 0xFFFFFFFF), (2, 1)], [1, 1, 0.0, 1.0]),
    ("zero-width", [(255, 1)], [0, 720, 0.0, 1.0]),
    ("zero-height", [(255, 1)], [1280, 0, 0.0, 1.0]),
    ("dimensions-wrap", [(255, 65536)], [65536, 65537, 0.0, 1.0]),
    ("dimensions-wrap-zero", [(255, 1)], [65536, 65536, 0.0, 1.0]),
]


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Use: python make_synthetic.py NEW_OUTPUT_DIRECTORY")
    manifest = json.loads(Path(__file__).with_name("cases.json").read_text())
    known = {case["id"]: case for case in manifest}
    output = Path(sys.argv[1])
    output.mkdir(parents=True, exist_ok=False)
    for name, occupied, params in CASES:
        bins = [0] * 256
        for index, count in occupied:
            bins[index] = count
        data = {
            "histogram": struct.pack("<256I", *bins),
            "uniform": struct.pack("<IIff", *params) + bytes(16),
            "output": struct.pack("<I", known[name]["native_output_bits"]),
        }
        case_dir = output / name
        case_dir.mkdir()
        for part, raw in data.items():
            key = "native_output_sha256" if part == "output" else f"{part}_sha256"
            if hashlib.sha256(raw).hexdigest() != known[name][key]:
                raise SystemExit(f"Case identity mismatch: {name}/{part}")
            (case_dir / f"session-0-{part}.bin").write_bytes(raw)
    print("Wrote nine synthetic cases with saved native-oracle output bits")


if __name__ == "__main__":
    main()
