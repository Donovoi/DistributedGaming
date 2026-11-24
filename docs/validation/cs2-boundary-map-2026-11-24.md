# Selected pass boundary and cost map

Date: 2026-11-24. Location: remote PC A. Progress: 50%. M5 remains pending.

The selected reduction's result is copied to a separate four-byte buffer. Its eventual consumer and completion rules are still unknown. CPU consumption has not been established. Do not insert a remote result at this boundary yet.

This is static analysis of an existing capture, not a new benchmark. RenderDoc's `CaptureFile.GetStructuredData` exposed recorded API metadata without creating a GPU replay controller. The GPU workload stayed running. The [machine record](cs2-boundary-map-2026-11-24.json) pins the capture, RenderDoc source, extraction helper, and private map.

## Recorded resources

Hypothesis: the small pass has a clear input/output boundary that could support a bounded replacement. The current map identifies part of that boundary. It does not establish safe replacement.

| Resource | Buffer bytes | Memory offset | Observation |
| --- | ---: | ---: | --- |
| Histogram | 1,024 | 32,111,872 | Written by E6901; used by E6905 |
| Reduction output | 4 | 32,112,896 | Written by E6905; copied by E6908 |
| Copy destination | 4 | 0 | Separate memory allocation; eventual consumer unknown |
| Uniform pool | 2,097,152 | 0 | The pass uses only a 32-byte range at offset 967,104 |

The histogram and output share one allocation. Their recorded buffer ranges are adjacent and do not overlap: the histogram ends where the output starts. A scan used the recorded memory-requirement extent for both sides of each comparison, including the output's 256-byte requirement and the copy destination's 16-byte requirement. It found no overlap candidate with other recorded buffer or image bindings for the four listed resources. This is not proof about resource lifetimes, device-address accesses, or later frames.

The copy moves four bytes from offset zero to offset zero. Its primary command buffer is marked for one-time submission. It is the last of ten command buffers in one recorded submission batch. That batch has one wait semaphore, one signal semaphore, and a fence. The wait stage is `COLOR_ATTACHMENT_OUTPUT`; it must not be described as protecting every compute or copy operation.

No mapping, unmapping, flush, or invalidate call for these allocations was found in the inspected structured metadata. This does not prove that no CPU access occurred. A persistent mapping may have been established earlier, and ordinary CPU loads through its pointer do not call Vulkan. Capture event numbers and resource IDs identify this record only. They are not runtime identities.

## Cost decision

The selected E6905 dispatch took 3.072 microseconds in one earlier GPU profile. The highest individual event in that profile was E4520 at 52.224 microseconds. Neither value is a repeated timing distribution or a whole-pass/whole-game bound. The critical path has not been established.

Sending just one 512-byte half histogram over a 1 Gb/s link requires at least 4.096 microseconds of payload serialization. That excludes GPU readback, headers, latency, remote dispatch, return traffic, merging, upload, and added synchronization. The proposed synchronous per-job transfer design has no credible benefit margin for this pass under the recorded configuration. Keep it as a small integration check; do not choose it as the performance target.

## Required next work

1. Observe live runtime identities without replacing work. Match the shader hash and pipeline state. Track resource generations, views, descriptor updates, offsets, and ranges across several offline frames.
2. Identify the copy destination's owner, mapping lifetime, completion mechanism, and permitted access interval. API observation alone cannot reveal ordinary CPU loads through an already mapped pointer.
3. Establish a one-PC replacement only after those rules are known. Keep one owner of the output. A late worker result must not overwrite a local fallback result.
4. Add a two-PC path only after the local path is correct. Check integer partials and their combined wrapping sum separately from the final float. Add bins 127 and 128, and reject missing, duplicate, overlapping, or stale partitions.
5. Select performance work using measured critical-path savings and total transfer cost. Count changing resources even when static assets are cached.

Intercepting `vkCmdDispatch` observes command recording. It does not provide completed producer data. Splitting work must preserve command-buffer state, descriptor consumption rules, submission dependencies, and consumer visibility. The unresolved consumer contract blocks live read/writeback now. No runtime code or game data was changed in this experiment.

Sources: [Vulkan command buffers](https://docs.vulkan.org/spec/latest/chapters/cmdbuffers.html), [queue submission rules](https://docs.vulkan.org/refpages/latest/refpages/source/vkQueueSubmit.html), and [descriptor rules](https://docs.vulkan.org/spec/latest/chapters/descriptorsets.html).
