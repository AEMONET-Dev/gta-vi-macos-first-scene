# Upstream mesh and pixel translation check

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run.

Observed 2026-11-25 on Apple Silicon. The translator source matches `pragyaangaur/VIctory-macOS@b6dd0b184277fc307b93a4cc90d23dca9f05e7b3`. The local native debug library came from worktree commit `76099d93151b33cac8965f9df0057cf9322e5370`. The shader crate has no source differences between those two commits.

Each row names a captured mesh and its paired pixel shader by the first 16 hex digits of its BLAKE3 container hash. All 17 distinct inputs matched those identifiers locally. The standalone column is the byte count returned by `emit::mesh_outputs` on the standalone mesh DXIL from `emit::translate_with`. The linked column uses the mesh DXIL from `emit::translate_linked(mesh, Some(pixel), options)`, explicitly invoked for every pair. Each linked call returned both mesh and pixel outputs. All nine pairs used `view_marks=true`, `atomic_locks=true`, `bias_as_gradients=true`, `host_wave32=true` and `narrow_waves=true`.

| Local observation used for sample selection | Mesh | Pixel | Standalone estimated bytes | Linked estimated bytes |
| --- | --- | --- | ---: | ---: |
| Local v3 warm, 34,816-byte rejection | b68376181e9c8b2f | b3d05023ed08e15e | 31,744 | 28,160 |
| Local v3 warm, 34,816-byte rejection twice | 374cff5e94fee812 | 81f23cdc52f59a69 | 31,744 | 28,160 |
| Local v3 warm, 34,816-byte rejection | ba9865c71b8d1947 | a6e15c9b7799fbf5 | 31,744 | 28,672 |
| Local v3 warm, 34,816-byte rejection | c3ba22ec6fc5dd4a | ac0d65016e7d9341 | 28,160 | 22,528 |
| Local v2 warm input mismatch | 792314eae4f7da77 | 7bdefdfde461f6e3 | 29,696 | 27,648 |
| Local v2 warm input mismatch | 3e6992c064c72c10 | ee41ebc3eb3fbbdc | 30,208 | 28,160 |
| Earlier close-up mesh investigation | 093c64546c55554e | 818a9a0ec6a53abb | 28,672 | 27,136 |
| Earlier close-up mesh investigation | da22b2adfd4f76d3 | d4f7cbbbf181e452 | 28,672 | 28,160 |
| Earlier close-up mesh investigation | da22b2adfd4f76d3 | f87e29908986f4d8 | 28,672 | 28,672 |

For the local driver failures, the pair is attributed by the host log immediately following the driver rejection in single-worker mode. The local game logs and the upstream offline translation used different renderer sources. The upstream runtime automatically tries linked translation when `VICTORY_LINK_MESH_OUTPUTS=1` and its own standalone estimate exceeds 32,768 bytes. All nine upstream estimates fall below this threshold. This check exercised DXIL emission and software size estimates only. Driver pipeline creation, draw results and timings on upstream remain unmeasured.

[Upstream paired translation PR](https://github.com/pragyaangaur/VIctory-macOS/pull/161) · Local v3 warm log (`victory-run-agpackv3-warm-20261125-214932-h8x9t9mp`) · Earlier local mismatch log (`victory-run-agxpc3-warm-20261125-212703-g26fk_wz`)
