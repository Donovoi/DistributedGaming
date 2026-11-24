# Research method

1. State the question and cite the evidence.
2. Write a hypothesis that a test can disprove.
3. Let a detractor challenge the hypothesis and test.
4. Build the smallest useful experiment.
5. Keep raw results private. Publish needed, redacted evidence.
6. Let a second detractor inspect the code and results.
7. Accept, reject, or revise the hypothesis. Update README progress.

This follows the research-and-experiment loop described in [FutureHouse Robin](https://arxiv.org/abs/2505.13400). Robin was studied for scientific discovery. It is not a game reversing tool. No Robin service or model API was used for this PR.

## Experiment H1

Hypothesis: a bounded Rust TCP harness can split a fixed render job across the five available PCs and reconstruct the exact output.

Test: run two scene configurations. Compare all frames, including warmups, against a full-frame local render. Check the release binary hash on every PC.

Result: H1 passed. See [validation](validation/2026-11-24.md). This result concerns a fixed CPU scene only.

## Experiment H2

Hypothesis: equal row partitions across the five PCs will reduce completion time against a local run with five render threads.

Result: H2 failed in both smoke tests. Distributed medians were higher. More profiling is needed to separate compute imbalance, copies, connection costs, and network cost. Do not assign the entire slowdown to the network without measurement.

## Primary sources

- [gVulkan, USENIX ATC 2024](https://www.usenix.org/system/files/atc24-gu-yicheng.pdf): API interception and shader rewriting for ray-tracing partitions. Its four-GPU result came from a cloud server, not these five PCs.
- [Chromium](https://graphics.stanford.edu/papers/cr/): graphics command streams across clusters.
- [Kahawai](https://www.microsoft.com/en-us/research/publication/kahawai-high-quality-mobile-gaming-using-gpu-offload/): local and remote game rendering with engine changes.
- [Distributed light probes](https://arxiv.org/abs/2103.05875): compact lighting updates can decouple remote work from display updates.
- [Vulkan device groups](https://docs.vulkan.org/guide/latest/extensions/device_groups.html): local device grouping does not provide our LAN transport.
- [Rust TCP streams](https://doc.rust-lang.org/std/net/struct.TcpStream.html): socket timeout behavior. The harness adds an absolute deadline.

## Next hypothesis

A selected game pass with small outputs can save more compute time than transfer and merge cost on this network. First measure a captured pass. Do not patch a game solely because a model predicts a benefit.
