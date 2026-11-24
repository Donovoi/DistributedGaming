"""Check the fixed milestone ledger and the README progress block."""
import argparse
import json
from pathlib import Path
import re
import subprocess

START = "<!-- progress:start -->"
END = "<!-- progress:end -->"


def progress_block(root):
    data = json.loads((root / "progress.json").read_text(encoding="utf-8"))
    milestones = data["milestones"]
    if not milestones or len({m["id"] for m in milestones}) != len(milestones):
        raise ValueError("Milestone IDs must be unique and nonempty.")
    for m in milestones:
        if type(m["weight"]) is not int or m["weight"] <= 0:
            raise ValueError("Weights must be positive integers.")
        if m["status"] not in ("pending", "complete"):
            raise ValueError("Status must be pending or complete.")
        if not m.get("acceptance", "").strip():
            raise ValueError("Each milestone needs an acceptance check.")
        if m["status"] == "complete":
            if not m.get("evidence"):
                raise ValueError("A complete milestone needs evidence.")
            for entry in m["evidence"]:
                path = (root / entry).resolve()
                if not path.is_relative_to(root.resolve()) or not path.is_file():
                    raise ValueError("Evidence must be an existing file inside this repo.")
    if sum(m["weight"] for m in milestones) != 100:
        raise ValueError("Milestone weights must total 100.")
    points = sum(m["weight"] for m in milestones if m["status"] == "complete")
    rows = [START, f"Goal progress: **{points}%** ({points}/100 accepted milestone points).", "",
            "This is a checklist score. It is not an estimate of time, effort, or game speedup.", "",
            "| Step | Acceptance area | Points | State |", "| --- | --- | ---: | --- |"]
    rows.extend(f'| {m["id"]} | {m["title"]} | {m["weight"]} | {m["status"]} |' for m in milestones)
    rows.append(END)
    return "\n".join(rows)


def check_readme_changed(root, base):
    if not re.fullmatch(r"[0-9a-fA-F]{40}", base):
        raise ValueError("Base must be a full commit SHA.")
    names = subprocess.check_output(
        ["git", "diff", "--name-only", base, "HEAD", "--", "README.md"], cwd=root, text=True
    ).splitlines()
    if "README.md" not in names:
        raise ValueError("Every PR must update README.md, including progress and checks.")


def check_fixed_plan(root, base):
    if not re.fullmatch(r"[0-9a-fA-F]{40}", base):
        raise ValueError("Base must be a full commit SHA.")
    # A missing file is valid only for the first ledger. Other git errors must fail.
    names = subprocess.check_output(["git", "ls-tree", "--name-only", base, "--", "progress.json"], cwd=root, text=True)
    if not names.strip():
        return
    before = json.loads(subprocess.check_output(["git", "show", f"{base}:progress.json"], cwd=root, text=True))
    after = json.loads((root / "progress.json").read_text(encoding="utf-8"))
    def plan(data):
        return data["goal"], [(m["id"], m["weight"], m["acceptance"]) for m in data["milestones"]]
    if plan(before) != plan(after):
        raise ValueError("The goal, milestone IDs, weights and acceptance checks must stay fixed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--base")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    block = progress_block(root)
    path = root / "README.md"
    text = path.read_text(encoding="utf-8")
    if text.count(START) != 1 or text.count(END) != 1 or text.index(END) < text.index(START):
        raise ValueError("README needs one ordered progress marker pair.")
    expected = text[:text.index(START)] + block + text[text.index(END) + len(END):]
    if args.write:
        path.write_text(expected, encoding="utf-8", newline="\n")
    elif text != expected:
        raise ValueError("README progress is stale. Run python3 scripts/check_progress.py --write.")
    if args.base:
        check_readme_changed(root, args.base)
        check_fixed_plan(root, args.base)
    print("Progress checks passed.")


if __name__ == "__main__":
    main()
