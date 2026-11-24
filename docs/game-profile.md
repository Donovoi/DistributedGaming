# Proposed game profile contract

Status: design only. The current binary does not load this profile or execute patches.

## Required evidence

1. Identify the game, build, operating system, graphics API, and executable SHA-256.
2. Identify the captured frame, pass, shaders, and resource hashes.
3. List each input. State its owner, size, format, update rule, and lifetime.
4. List each output. State its owner, size, format, and next reader.
5. List every dependency. Include GPU writes, indirect addresses, atomics, history buffers, barriers, and CPU readback.
6. State the patch and its allowed preconditions. Include original and changed shader hashes.
7. Provide a local replay command and expected output. Define any allowed error before the test.
8. Provide split replay tests. Include scene motion, boundaries, resource changes, and failure cases.
9. Measure compute, transfer, merge, and end-to-end time. Keep sample data.
10. Record two detractor rounds and the final decision.

Use stable identifiers instead of raw pointers. Keep game assets and account data outside this repo. Never treat a file path, shell command, or code snippet from a model as approved work.

## Admission rule

Reject the proposal if any dependency is unknown, any input lacks an owner, any identity check fails, or any replay differs beyond the declared tolerance. Reject it if it has no measured benefit for the intended link. A local replay result does not prove a network result.

An LLM may narrow the search to a few costly passes. The harness can apply the same checks to each candidate. A complete decompilation of the game may not be needed. This is a hypothesis. No time saving from LLM reversing has been measured yet. No model calls or credentials are needed for the current harness.
