# Work rules

## Goal

Run one supported game across at least two physical PCs. Keep the same game settings and image quality. Measure the result against the fastest participating PC running alone. Report frame time and input delay separately.

## Every pull request

1. Update README.md. State the change, checks, limits, next work, and goal percentage.
2. Update progress.json only when an acceptance check is complete. Do not change weights to raise the percentage.
3. Run the relevant tests. Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --all-targets --locked` for Rust changes.
4. Run `python3 scripts/check_progress.py`. CI also checks that the PR changes README.md.
5. Use two detractor rounds. First challenge the design. Then use an independent reviewer to challenge the code and evidence. Record findings, fixes, and remaining limits in docs/reviews/.
6. Do not call a simulation, loopback test, or CPU test scene a game speedup.

## Writing

Use short sentences and simple English. Put the action first. Use one name for each part. Give numbered steps for procedures. State expected results. State a limit next to the claim it limits. Avoid sales language.

## Research and LLM output

Record the source, hypothesis, experiment, result, and decision. Use primary sources. An LLM may propose a game profile or patch. The harness must check it. Unknown data ownership or missing dependencies block use of that patch. A valid JSON file is not proof of a correct patch.

## User data

Keep account IDs, private IP addresses, SSH details, raw Steam library data, and secrets out of commits and PRs. Store private run files under ignored runs/ or outside the repo. Commit only the needed, redacted evidence. Do not collect credentials.
