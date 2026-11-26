# First-scene rendering and frame-time proposal for Apple Silicon

The proposal is open for discussion in [VIctory-macOS issue #208](https://github.com/pragyaangaur/VIctory-macOS/issues/208).

Our local VIctory renderer reaches the GTA VI opening street scene on an M5 Max. Faces and clothing are visible in sampled close-ups. Facial lighting, foliage and some mesh pipelines still need work. At 1080p with the Series S configuration, our instrumented picture baseline took 2,733 ms per delivered frame at the median and 4,195 ms at P95 over 23 intervals with diagnostic captures. The 30 FPS target gives us 33.3 ms per frame. The experimental branches build on an older VIctory-macOS revision. Our upstream comparison used `b6dd0b18`. [Study history](PROCESS.md)

## Picture

- Porting upstream's mesh-export ordering fix eliminated compiler-service interruptions in the measured local runs. Of 55 pipelines previously associated with a compiler crash, 54 were rebuilt in the warm comparison and all 54 succeeded. A further local change packed split generic output rows and preserved system values in the paired pixel inputs. Both previously mismatched combinations were exercised successfully. The warm run still logged five 34,816-byte mesh rejections across four combinations against a 32,768-byte limit. [Export ordering](https://github.com/pragyaangaur/VIctory-macOS/pull/167) · [Graphics notebook](notebooks/team/Antigravity.md)
- Nine relevant pairs translated offline with upstream's translator. Its standalone size estimates for all nine were below the automatic linking threshold. The next graphics comparison should record both the estimated size and the actual Metal pipeline creation result for each pair, with linking enabled and disabled. Two native caches matter here, the VIctory translator cache and the D3DMetal driver cache. [Upstream pairing](https://github.com/pragyaangaur/VIctory-macOS/pull/161) · [Paired results](notebooks/latency/upstream-linked-mesh-20261125.md)
- In two separate runs of the local Wave64 port, strict translation refused 71 compute shaders, including the deferred-lighting shader. A permissive configuration reported zero refusals, but used a different worker count and saw a different workload. A matched run should verify writes to the light target and inspect skin, eyes, shadow detail and foliage in the final frame at the same camera position. [Wave64 work](https://github.com/pragyaangaur/VIctory-macOS/pull/180) · [Fragment-stage investigation](https://github.com/pragyaangaur/VIctory-macOS/issues/203)

A subsequent local run also enabled the #148 shared-memory barriers and PC FSR 3.1.6. The warm run logged zero translator refusals and five mesh-limit errors. Temporal upscaling smeared dark purple blocks across faces. The GPU-stage timing changed alongside several settings. A controlled comparison should toggle the barriers and FSR independently. [Follow-up graphics record](notebooks/team/Antigravity.md)

## Performance

| Measurement | Local observation | Interpretation |
| --- | --- | --- |
| Low-capture crossover, original read path, 16 intervals | Median delivered frame 2,483 ms. Median buffer refresh 1,260 ms. The largest batch took 429 ms at the median. | Time reads, retries, mapping queries and remaining work on the worker that delays the submission. |
| Scoped write tracking, same picture-textures baseline, two runs per arm, 40 intervals each | Reference frame medians 3.136 and 2.795 s. Candidate medians 2.733 and 2.386 s. The 174 MiB resource fell from about 145 to 2.9–3.4 ms of worker time per frame. | Draw counts and run order varied. The timing benefit for this resource is established locally. The causal effect on displayed frames needs a matched comparison. |
| Partial mip decoding | 196 synthetic layouts produced byte-identical output. Two observed layouts took 17.75 and 9.69 µs, down from 88.89 and 111.33 µs. | Retain the direct path for resident levels and measure how often it runs during the common scene window. |

The scoped tracker audit matched 2,190,352 page comparisons. It skipped 1,780,219 observations under pending GPU writes. Aliases, remaps, same-frame CPU writes and GPU writeback all need coverage before broadening tracking. Upstream has independent work on copy statistics, heap locking, changed texture subresources and zero-copy title buffers. Our local four-worker implementation and public main have different batch structures. [Timing and audit](notebooks/team/Claude.md) · [Upstream statistics](https://github.com/pragyaangaur/VIctory-macOS/pull/193) · [Heap locking](https://github.com/pragyaangaur/VIctory-macOS/pull/199) · [Texture refresh](https://github.com/pragyaangaur/VIctory-macOS/pull/176) · [Zero-copy study](https://github.com/pragyaangaur/VIctory-macOS/issues/204)

A later synthetic test of #193 recorded 288,000 range observations on four workers. One run spent 78.6 ms recording to the shared map and 7.7 ms with worker-local lists. The timer ended before merging the lists. Aggregation cost and scene-level impact remain open. [Performance source](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/renderer-performance)

## Proposed comparison

1. Establish an upstream baseline with a fixed opening camera window. Record both executable hashes, graphics switches, worker count and cache state. Count actual pipelines reached, delivered frames, draws and dispatches. Use separate runs for image diagnosis and low-capture timing.
2. Test the outstanding mesh/pixel pairings and Wave64 lighting with the same workload. Check the producer's output, the consuming pass and the final picture. Use a small synthetic case for any newly isolated translator change.
3. Measure timer overhead with per-worker nanosecond counters published once per batch. Compare original reads, scoped tracking and resident-mip decoding independently. Report P50 and P95 frame times, batch wall time and completed work.
4. Compare baseline, graphics-only, performance-only and integrated variants under matching conditions and alternate run order. Accept a speed claim when repeated runs improve delivered frame times with comparable workloads and preserved visual output. The present integration has build and offline shader-test coverage. [Sources and tests](REPRODUCE.md)
