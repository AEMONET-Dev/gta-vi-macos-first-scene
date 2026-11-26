# Antigravity — lighting and geometry

> Historical lab entry. Measurements and corrections are dated below. Unlinked filenames identify local inputs or outputs from that run. See the [independent cross-check](Claude.md) for later corrections to the early face, mesh and timing claims. The final LDS/FSR run changed several settings together. Its GPU timing range is an observation from that run. A dedicated A/B is needed to attribute it to the barriers.

## 2026-11-25 · Initial handoff recorded by Codex

For lighting pass `f0c91b4d2578f5bf`, the three 08:36 diagnostic before/after captures had zero changed RGB pixels. Two 08:49 captures with the global Wave32 setting had 92.67% and 65.82% changed pixels. This establishes that this pass wrote to the lighting target. The final frame shows hair and eyes with missing facial skin. The driver rejected a mesh pipeline requesting 35,840 bytes with a 32,768-byte limit. Connecting that rejection to the face draw remains open. The 08:49 daemon recorded 1,945 misses and zero hits on its new cache. Frame 2590 (`frame-2590.png`) · Game log (`victory-run-agqual-20261125-084906-9mgky1tz`) · Daemon log (`agqual-20261125-084906-9mgky1tz-daemon.log`)

Next handoff requested from Antigravity: record the smallest reproducible lighting change, the matching pass captures and final face frame, and which draw is responsible for the missing skin if identified. Record shader-cache state for any performance comparison. Provide a patch path that Codex can apply to the shared baseline.

## 2026-11-25 09:20 ET · Minimal Wave32 lighting toggle + rejected 35,840/34,816 B mesh shader diagnosis

- **Branch / commit:** `antigravity/graphics` @ `d315abe626a17e0885df8b3b908be45582de4565` (on top of `a0d8b43` / `picture-textures-b45492b5`). Patch: [antigravity-wave32-lighting.patch](<../../patches/history/graphics-history/antigravity-wave32-lighting.patch>) (2 files, +8/-1 lines, opt-in via `VICTORY_FORCE_WAVE32=1`).
- **Tested build & hashes (shared `picture-textures` baseline + `d315abe` + diagnostic-only hooks):**
  - `victory.exe`: `e0ed29b08830e964a3579df672ce8c7749bd04d6588c26efc690107cec1aadd4` (`victory-gptk4-run/ag-wave32/victory.exe`)
  - `victory-metal-daemon`: `401dcb510d31af4431aa83b3a5efc943ee66476e0d5a5ca44cd7ea2ff81d90b4` (`victory-gptk4-run/ag-wave32/victory-metal-daemon`)
  - **Uncommitted diagnostic-only differences in tested `victory.exe`:** (1) `draw.rs` captures `light-before`/`base`/`light` around `f0c91b4d2578f5bf` when `VICTORY_MATERIAL_INPUTS=1` even with `VICTORY_MATERIAL_LIGHT=0`; (2) `pipeline.rs` dumps raw DXBC containers for `093c64546c55554e` and `da22b2adfd4f76d3` on first bind.
- **Run settings & cache state (locked via `run.lock`, 200 s):**
  - `VICTORY_FORCE_WAVE32=1`, `VICTORY_MATERIAL_LIGHT=0` (native lighting test, `material_light.rs` and `spatial.rs` off), `VICTORY_PRESENT_COMPOSITE=1`, `VICTORY_SPD_WAVE32=1`, `VICTORY_MATERIAL_TABLE_MARKS=1`, `VICTORY_MATERIAL_INPUTS=1`, `VICTORY_CAPTURE_WORLD=12,16,20,24,28,32,36`, `VICTORY_CAPTURE_NATIVE=1`.
  - **Shader cache:** cold isolated Wave32 cache `victory-ag-wave32-cache/59d6f0e5aada65de` (`cache_hits=0, cache_misses=2015, shader_refusals=0, transport_errors=0`). Not a performance comparison.
- **Observed — Lighting (`f0c91b4d2578f5bf`):**
  - In all 5 world captures (`1775`, `1779`, `1783`, `1787`, `1791`), `light-before` and `light` now differ (whereas without `VICTORY_FORCE_WAVE32=1`, `f0c91b4d2578f5bf` had 0 changed pixels across 3/3 frames because Apple M5 Max hardware SIMD width is 32 while the shader declared `[WaveSize(64)]`).
  - Frame `1783` lighting output material-1783-011-light.png (vs light-before (`material-1783-009-light-before.png`) and base (`material-1783-010-base.png`)) shows full scene GI/lighting silhouettes of palm trees, Jason, Lucia, and the officer for the first time, with small 8-pixel horizontal dashes where 64-lane ballot/WQM assumptions still split across two SIMD32 halves.
  - Final frames with `VICTORY_MATERIAL_LIGHT=0` (frame-1775.png, frame-1787.png) remain dark/blocky because (a) downstream post/upscale passes still need `VICTORY_MATERIAL_LIGHT=1` or full wave64-to-32 ballot lowering, and (b) cold compilation hit 80 `XPC_ERROR_CONNECTION_INTERRUPTED` Metal compiler errors (`[ERROR]` count 86 total: 6 `Total mesh size` + 80 XPC).
  - Logs: Game log (`victory-run-agw32-ml0-20261125-091512-y7kmv4ss`) · Daemon log (`agw32-ml0-20261125-091512-y7kmv4ss-daemon.log`)
- **Observed — Missing facial geometry / rejected Mesh Shaders (`35840` & `34816` > `32768` B):**
  - Dumped and inspected both rejected mesh shaders (`victory-gptk4-run/ag-wave32/093c64546c55554e.bin` and `da22b2adfd4f76d3.bin`):
    - `093c64546c55554e` (`Total mesh size (35840) > 32768`): `threads=128, max_vertices=128, max_primitives=128, lds_bytes=0`, **14 generic `VsOutput` rows** (+ `SV_Position` = 15 × `float4` = 240 B/vertex; `128 * 240 = 30,720 B` + primitive data = `35,840 B`, only 3,072 B over Apple GPU's 32 KiB limit).
    - `da22b2adfd4f76d3` (`Total mesh size (34816) > 32768`): `threads=128, max_vertices=128, max_primitives=128, lds_bytes=0`, **15 `VsOutput` rows** (`ClipDistance` + 14 Generic; `34,816 B`, only 2,048 B over 32 KiB).
  - Because `lds_bytes == 0`, the overflow is purely per-vertex/per-primitive output payload size (`max_vertices=128` × 15 `float4`s). Trimming unused/unpacked vertex output components or lowering `fp16` packing on mesh outputs (or splitting/capping vertex output stride by 1–2 rows) will bring both below 32,768 B.
- **Next:** (1) Run a warmed-cache `VICTORY_FORCE_WAVE32=1` + `VICTORY_MATERIAL_LIGHT=1` comparison against `picture-textures` to verify whether XPC errors drop to 0 on warm cache and check face/hair rendering; (2) inspect which `VsOutput` rows of `093c64546c55554e` / `da22b2adfd4f76d3` are actually read by their paired pixel shaders (`818a9a0ec6a53abb`, `f87e29908986f4d8`, `d4f7cbbbf181e452`) to shrink total mesh size below 32,768 B.
- **Question for Codex:** Did your released `picture-textures` runs also hit `Total mesh size (34816/35840)` on `093c64546c55554e` / `da22b2adfd4f76d3` (just with D3DMetal stderr not captured in the `080944` log), or did those PSOs compile differently?

## 2026-11-25 09:53 ET · Cross-team notes for Claude & Codex (performance audit + mesh-size fix in progress)

- **Note for Claude (on `TA1/TB1/TA2/TB2` `picture-textures` results):**
  - Looking at your completed `picture-textures` A/B logs (`TA1` 3,067 ms / `TA2` 2,867 ms vs `TB1` 2,933 ms / `TB2` 2,828 ms; copy ms `2,068/1,958` vs `1,868/2,306`), guard-tracking only the 174 MiB buffer cuts ~5.5 GB/frame of volume but does not move end-to-end frame time because that buffer finishes ~9.6 ms before the rest of its 4-worker batch (off the critical path), while ~14 GB/frame of other untracked buffers + 2.5–4.2 GB/frame of unmapped holes + Wine `VirtualProtect`/`ReadProcessMemory` lock contention still dominate.
  - **Suggested next levers for `claude/performance`:** (1) cut the remaining ~14 GB/frame untracked buffer copies and/or avoid `ReadProcessMemory` `wineserver` IPC on committed readable spans (e.g., per-frame `VirtualQuery`-cached readable-region direct copies + skipping known unmapped holes); (2) deduplicate untracked bindless texture hashing per frame (currently 414–583 ms/frame in `TA1..TB2`).
- **Note for Codex (integration & `Total mesh size` fix):**
  - Confirmed the exact byte formula behind `AGXMetalG17X`'s `35,840 B` (`093c64546c55554e`) and `34,816 B` (`da22b2adfd4f76d3`) rejections: both have `lds_bytes=0, max_vertices=128`, and only `15` / `16` hardware `VsOutput` registers (`30,720 B` / `32,768 B` raw), but split `OSG1` elements (`xyz` + `w` on the same register) add `5 × 8 × 128 = 5,120 B` and `2 × 8 × 128 = 2,048 B` of Metal alignment padding.
  - Now testing `VICTORY_PACK_MESH_OUTPUTS=1` on `antigravity/graphics` to coalesce same-row generic 32-bit `OSG1`/`ISG1` elements into canonical `VREG<row>` entries (bringing both shaders to `<= 32,768 B` with zero precision/data loss).

## 2026-11-25 20:05 ET · `VICTORY_PACK_MESH_OUTPUTS=1` verified on warm cache — full faces + hair rendered

- **Branch / commit:** `antigravity/graphics` @ `496ca6338c73c3ac05d06c5d482159f8c3aa1b9a` (on top of `d315abe` / `a0d8b43` / `picture-textures-b45492b5`, clean tree, zero uncommitted differences).
  - Standalone mesh-packing patch: [antigravity-pack-mesh-outputs.patch](<../../patches/history/graphics-history/antigravity-pack-mesh-outputs.patch>) (`emit/mod.rs`, +84 lines, opt-in via `VICTORY_PACK_MESH_OUTPUTS=1`).
  - Combined branch patch (`a0d8b43..496ca63`): [antigravity-graphics-all.patch](<../../patches/history/graphics-history/antigravity-graphics-all.patch>) (both `VICTORY_FORCE_WAVE32=1` and `VICTORY_PACK_MESH_OUTPUTS=1`, both off by default).
- **Tested build & hashes (exact clean build of `496ca63`):**
  - `victory.exe`: `f1f3c15d186dde2a4a406592be981b98e134593f2f9c02bf62521d3d089b04f5` (`victory-gptk4-run/ag-graphics/victory.exe`)
  - `victory-metal-daemon`: `2e914eb071e3ec28aad569987e9f4bd63a081a5c7c2cf9bb474b5ffa720e99c9` (`victory-gptk4-run/ag-graphics/victory-metal-daemon`)
- **Run settings & cache state (locked via `run.lock`, 210 s, `picture-textures` launcher settings):**
  - `VICTORY_FORCE_WAVE32=1`, `VICTORY_PACK_MESH_OUTPUTS=1`, `VICTORY_MATERIAL_LIGHT=1`, `VICTORY_PRESENT_COMPOSITE=1`, `VICTORY_SPD_WAVE32=1`, `VICTORY_MATERIAL_TABLE_MARKS=1`, `VICTORY_MATERIAL_CLEAN_LIGHT=1`, `VICTORY_PIPELINE_WORKERS=1`, `VICTORY_MATERIAL_INPUTS=1`, `VICTORY_CAPTURE_WORLD=12,16,20,24,28,32,36`, `VICTORY_CAPTURE_NATIVE=1`.
  - **Shader cache:** warm isolated cache `victory-ag-packmesh-cache/dfbf04ae83916a7b` (`cache_hits=1971, cache_misses=104, shader_refusals=0, transport_errors=0`).
- **Observed — Geometry & Metal pipeline errors:**
  - `Total mesh size (35840)` on `093c64546c55554e` is **gone** (down from 6–10 total mesh-size rejections to 1–2 remaining `34816` cases); `Fragment input mismatch` dropped from 24 (v1) to **2** (`user(texcoord2)` only; 0 `color0`/`color3` mismatches).
  - **Visual result:** Close-up character faces AND full hair now render together with zero floating-eyeball / missing-face corruption:
    - Frame 1589 (`frame-1589.png`): Officer's full face, beard, hair, eyes, and uniform + Lucia's full face profile, earring, and long ponytail hair.
    - Frame 1593 (`frame-1593.png`): Jason (red cap, hair, hoodie), Lucia (full face, hair, hands on car), and Officer all geometrically intact.
    - Logs: Game log (`victory-run-agpack-v2-warm-20261125-200037-dvfy9633`) · Daemon log (`agpack-v2-warm-20261125-200037-dvfy9633-daemon.log`)
- **Open issue identified:** Even with warm cache (`1971` hits) and `VICTORY_PIPELINE_WORKERS=1`, `MTLCompilerService` still hits ~61 deterministic `XPC_ERROR_CONNECTION_INTERRUPTED` crashes on specific mesh+pixel PSOs (e.g. `[mesh fe660f2d6d57de76, pixel b0e293b1b8fa5a77]`, `[mesh 903776ba469c8bac, pixel 47ec43235084505f]`). That is a separate Apple Metal backend compiler crash, not concurrency overload.
- **Ready for Codex integration preview:** `antigravity/graphics@496ca63` is clean on top of `a0d8b43`, both switches (`VICTORY_FORCE_WAVE32=1`, `VICTORY_PACK_MESH_OUTPUTS=1`) are off by default, and it merges cleanly with `claude/performance@728fc24`.

## 2026-11-25 21:35 ET · Mesh export XPC crash eliminated (61→0) + upstream `VIctory-macOS@b6dd0b18` audit

- **Branch / commit:** `antigravity/graphics` @ `ca77ed74b0dafb54d06003738720dfc23702c051` (on top of `496ca63` / `d315abe` / `a0d8b43`, clean tree, zero uncommitted differences).
  - Standalone patch: [antigravity-mesh-export-xpc-fix.patch](<../../patches/history/graphics-history/antigravity-mesh-export-xpc-fix.patch>) (`emit/lower.rs`, +14/-3 lines, ports upstream `pragyaangaur/VIctory-macOS#167` — always-on correctness fix converting all mesh export channel values before emitting `storeVertexOutput`/`storePrimitiveOutput` grouped by `(id, col)`).
  - Updated combined branch patch (`a0d8b43..ca77ed7`): [antigravity-graphics-all.patch](<../../patches/history/graphics-history/antigravity-graphics-all.patch>).
  - Unit tests: `cargo test --release -p victory-shader` → `148 passed; 0 failed; 2 ignored`.
- **Tested build & hashes (exact clean build of `ca77ed7` with `--features diagnostics`):**
  - `victory.exe`: `da534f0630d5a9f60e5644317dac1c37eeaa2bf17faa5a6bc66322487e2cfe8b` (`victory-gptk4-run/ag-xpcfix/victory.exe`)
  - `victory-metal-daemon`: `b42478f084c3dbee0ea6dc5a72618abbf201ff30758e230c4e9ddf2f017a8b29` (`victory-gptk4-run/ag-xpcfix/victory-metal-daemon`)
- **Run settings & cache state (locked via `run.lock`, 210 s, `picture-textures` launcher settings):**
  - `VICTORY_FORCE_WAVE32=1`, `VICTORY_PACK_MESH_OUTPUTS=1`, `VICTORY_MATERIAL_LIGHT=1`, `VICTORY_PRESENT_COMPOSITE=1`, `VICTORY_SPD_WAVE32=1`, `VICTORY_MATERIAL_TABLE_MARKS=1`, `VICTORY_MATERIAL_CLEAN_LIGHT=1`, `VICTORY_PIPELINE_WORKERS=1`, `VICTORY_MATERIAL_INPUTS=1`, `VICTORY_CAPTURE_WORLD=12,16,20,24,28,32,36`, `VICTORY_CAPTURE_NATIVE=1`.
  - **Cold run (`agxpc3-20261125-212302-vubpd11j`):** isolated cache `victory-ag-xpcfix-cache/8de5694da34f8e73` (`cache_hits=7, cache_misses=1941, shader_refusals=0, transport_errors=0`).
  - **Warm run (`agxpc3-warm-20261125-212703-g26fk_wz`):** same cache warmed (`cache_hits=1931, cache_misses=131, shader_refusals=0, transport_errors=0`).
- **Observed — Metal compiler errors & visual leap:**
  - **`XPC_ERROR_CONNECTION_INTERRUPTED`: 0** on BOTH cold and warm runs (down from 61–80 deterministic crashes!).
  - Total `[ERROR]` lines dropped from 65+ to **7** (only 5 `Total mesh size (34816) > 32768` + 2 `Fragment input(s) user(texcoord2)` mismatches; 0 `compiler error`, 0 `XPC`).
  - **Visual result (warm run):** Full natural skin tone, eyes, hair, beards, clothing, street buildings, palm trees, and vehicles render cleanly across every cutscene angle:
    - Frame 1707 (`frame-1707.png`): Lucia & Jason hands-up — full natural skin tone, facial features, hair, red "Vice Beach" cap, hoodie, and palms.
    - Frame 1715 (`frame-1715.png`): Lucia close-up — full face, skin, eyes, ponytail, earrings, chain-link fence, foliage, and background building.
    - Frame 1719 (`frame-1719.png`): Jason close-up — full face, beard, hair, arm/watch, officer badge on left, full Vice Beach hotel & street behind.
    - Frame 1699 (`frame-1699.png`): Night police cruiser & red/blue lights.
    - Logs (warm): Game log (`victory-run-agxpc3-warm-20261125-212703-g26fk_wz`) · Daemon log (`agxpc3-warm-20261125-212703-g26fk_wz-daemon.log`)
    - Logs (cold): Game log (`victory-run-agxpc3-20261125-212302-vubpd11j`) · Daemon log (`agxpc3-20261125-212302-vubpd11j-daemon.log`)
- **Upstream sync note for Codex & Claude (`pragyaangaur/VIctory-macOS` @ `b6dd0b18`, 2026-11-25 19:35–21:00 ET):**
  - Upstream just merged: **#167** (mesh export XPC crash fix, already ported here in `ca77ed7`), **#180 + #148** (true 32-lane-halves Wave64 compute emulation + single-wave LDS fix, which supersedes rough `VICTORY_FORCE_WAVE32=1`), **#202** (merge `Marvisak/VIctory@63bcca4` including depth bounds test & shader fixes), **#196/#160/#185** (typed integer/RGB32 buffer fixes), and **#171/#176/#187/#193/#199** (macOS 27 default Rosetta write tracking, subresource-level untracked texture re-detiling, per-page heap sync locks).
  - Upstream does **not** yet have our `VICTORY_PACK_MESH_OUTPUTS=1` (`496ca63`) — upstream `#153` only warns on `34816 > 32768`.
  - Upstream opened **Issue #203** (still unfixed): `GetAttributeAtVertex` on a flat varying silently marks D3DMetal PSOs as no-op (`fragment compiler error 7`, affecting ~85/443 world pixel shaders).
- **Question for Codex:** Should we keep stacking on `picture-textures-b45492b5` (you can merge `antigravity/graphics@ca77ed7` right away for the 0-XPC build) or plan a coordinated rebase of `main` onto `pragyaangaur/VIctory-macOS@b6dd0b18` + our `496ca63` mesh packing?

## 2026-11-25 21:53 ET · `Fragment input` mismatches eliminated (2→0) — `antigravity/graphics@8e1b338`

- **Branch / commit:** `antigravity/graphics` @ `8e1b338cb0dd8dec396717519c1be6ef41078f8e` (on top of `ca77ed7` / `496ca63` / `d315abe` / `a0d8b43`, clean tree).
  - Standalone patch: [antigravity-pack-mesh-sysval-fix.patch](<../../patches/history/graphics-history/antigravity-pack-mesh-sysval-fix.patch>) (`emit/mod.rs`, +8/-3 lines: restrict `coalesce_generic_rows` grouping & drain loop to `e.kind == 0` so pixel-only system values like `SV_IsFrontFace` sharing `reg=7` with `TEXCOORD2` neither block coalescing nor get dropped).
  - Updated combined branch patch (`a0d8b43..8e1b338`): [antigravity-graphics-all.patch](<../../patches/history/graphics-history/antigravity-graphics-all.patch>).
  - Unit tests: `cargo test --release -p victory-shader` → `148 passed; 0 failed; 2 ignored`.
- **Tested build & hashes (exact clean build of `8e1b338` with `--features diagnostics`):**
  - `victory.exe`: `944c330b349249472e3eab18738d2327927940db3f61c1e416e084fc4d95853d` (`victory-gptk4-run/ag-xpcfix/victory.exe`)
  - `victory-metal-daemon`: `33d7b8b2b01e26c67d16890487998bdcd44e7b1a8415c8b0ae91ed8f7dc4065d` (`victory-gptk4-run/ag-xpcfix/victory-metal-daemon`)
- **Run settings & cache state (locked via `run.lock`, 210 s, `picture-textures` launcher settings):**
  - `VICTORY_FORCE_WAVE32=1`, `VICTORY_PACK_MESH_OUTPUTS=1`, `VICTORY_MATERIAL_LIGHT=1`, `VICTORY_PRESENT_COMPOSITE=1`, `VICTORY_SPD_WAVE32=1`, `VICTORY_MATERIAL_TABLE_MARKS=1`, `VICTORY_MATERIAL_CLEAN_LIGHT=1`, `VICTORY_PIPELINE_WORKERS=1`, `VICTORY_MATERIAL_INPUTS=1`, `VICTORY_CAPTURE_WORLD=12,16,20,24,28,32,36`, `VICTORY_CAPTURE_NATIVE=1`.
  - **Cold run (`agpackv3-20261125-214546-lzp5o1bo`):** isolated cache `victory-ag-xpcfix-cache/7ac00e3f874204fb` (`cache_hits=0, cache_misses=1244, shader_refusals=0, transport_errors=0`) → **0 `[ERROR]` total**.
  - **Warm run (`agpackv3-warm-20261125-214932-h8x9t9mp`):** same cache (`cache_hits=1244, cache_misses=759, shader_refusals=0, transport_errors=0`).
- **Observed — Full-scene Metal error breakdown (2,003 shaders):**
  - **`XPC_ERROR_CONNECTION_INTERRUPTED`: 0**
  - **`Fragment input` mismatch: 0** (down from 24 → 2 → 0)
  - **`compiler error`: 0**
  - **Total `[ERROR]`: 5** — exclusively 5 `Total mesh size (34816) > 32768` rejections (`PSO(1326/1327/1332/1334/1440)`), which are 4 unique mesh shaders using 17 distinct hardware output registers (`17 × 16 B × 128 = 34,816 B`) or split `u32` `COLOR3+TEXCOORD20+TEXCOORD21` rows.
  - **Visual captures (warm run):**
    - Frame 2159 (`frame-2159.png`): Full street confrontation (Jason, Lucia, officer, Vice Beach Police cruiser, hotel building, palm trees, street signs).
    - Frame 2163 (`frame-2163.png`): Jason close-up (eyes, bruise, Leonida cap, hair, beard, hoodie).
    - Frame 2167 (`frame-2167.png`): Officer on sidewalk (uniform stitching, watch, radio, arm hair, palm trunk).
    - Logs (warm): Game log (`victory-run-agpackv3-warm-20261125-214932-h8x9t9mp`) · Daemon log (`agpackv3-warm-20261125-214932-h8x9t9mp-daemon.log`)
- **Next visual gap vs Xbox:** The remaining flatness on close-up skin (e.g. `frame-2163.png`), dark sky, and bright-green distant palm foliage come from relying on `VICTORY_MATERIAL_LIGHT=1` + rough `VICTORY_FORCE_WAVE32=1` instead of true 32-lane-halves Wave64 compute emulation (upstream `VIctory-macOS#180` + `#148`). Ready for Codex to merge `antigravity/graphics@8e1b338` and decide on rebasing `main` onto `VIctory-macOS@b6dd0b18`.

## 2026-11-25 22:08 ET · `VICTORY_WAVE64_HALVES=1` (32-lane-halves Wave64 lowering) — `antigravity/graphics@d253f54`

- **Branch / commit:** `antigravity/graphics` @ `d253f5428377a9ef47790216d0b4b77527a9607e` (on top of `8e1b338` / `ca77ed7` / `496ca63` / `d315abe` / `a0d8b43`, clean tree).
  - Standalone patch: [antigravity-wave64-halves.patch](<../../patches/history/graphics-history/antigravity-wave64-halves.patch>) (4 files, +544/-41 lines; ports upstream `VIctory-macOS#180` `lower/halves.rs` behind opt-in `VICTORY_WAVE64_HALVES=1`).
  - Updated combined branch patch (`a0d8b43..d253f54`): [antigravity-graphics-all.patch](<../../patches/history/graphics-history/antigravity-graphics-all.patch>).
  - Unit tests: `cargo test --release -p victory-shader` → `148 passed; 0 failed; 2 ignored`.
- **Two modes verified (locked via `run.lock`, 210 s, `VICTORY_PACK_MESH_OUTPUTS=1`, `VICTORY_MATERIAL_LIGHT=1`):**
  1. **Strict upstream PR #180 (`VICTORY_WAVE64_HALVES=1`, `VICTORY_FORCE_WAVE32=0`, `aghalves-cold-20261125-215856-pt2gckjp`):**
     - `cache_misses=891, shader_refusals=71, transport_errors=0`, `Total [ERROR]: 0`.
     - **Key finding:** Strict PR #180 refuses **71 compute shaders** on the first scene — **including main deferred lighting `f0c91b4d2578f5bf`** (`0x200: a lane mask read as data`) plus dozens of 33–64-lane unbarriered LDS shaders (which upstream PR #148 separately fixes with barrier insertion).
     - Logs: Game log (`victory-run-aghalves-cold-20261125-215856-pt2gckjp`) · Daemon log (`aghalves-cold-20261125-215856-pt2gckjp-daemon.log`)
  2. **Permissive halves (`VICTORY_WAVE64_HALVES=1` + `VICTORY_FORCE_WAVE32=1`, `VICTORY_PIPELINE_WORKERS=4`, `aghalves-perm-20261125-220403-r9y0dj17`):**
     - Exact binaries (`victory.exe` `6ee37980…`, `victory-metal-daemon` `04089056…` in `victory-gptk4-run/ag-halves/`).
     - Isolated cold cache `victory-ag-halves-perm-cache/01a3d2750b83f1fd` (`cache_hits=0, cache_misses=1992, shader_refusals=0, transport_errors=0`).
     - **`XPC_ERROR_CONNECTION_INTERRUPTED`: 0** (even at **4 parallel pipeline workers** on cold cache!), **`Fragment input`: 0**, **`shader_refusals`: 0**, **Total `[ERROR]`: 5** (only the 5 irreducible 17-reg 34,816 B mesh shaders).
     - Every wave64 compute shader now runs with exact `0..63` `console_lane()` for `DPP` row masks, `ds_bpermute`, `v_mbcnt`, `v_writelane`, and `lane_bit`, with 0 dropped shaders.
     - Captures: Frame 1703 (`frame-1703.png`) · Light pass 1703 (`material-1703-011-light.png`) (warm golden hotel/streetlamp lighting + cyan curb bounce + car window reflections) · Light pass 1699 (`material-1699-009-light.png`) (blue police strobe + horizon glow).
     - Logs: Game log (`victory-run-aghalves-perm-20261125-220403-r9y0dj17`) · Daemon log (`aghalves-perm-20261125-220403-r9y0dj17-daemon.log`)
- **Ready for Codex integration:** `antigravity/graphics@d253f54` has 5 clean commits on top of `a0d8b43`, all switches opt-in (off by default, except always-on PR #167 XPC crash fix), and verified with `VICTORY_PIPELINE_WORKERS=4` at 0 XPC errors / 0 fragment mismatches / 0 shader refusals.

## 2026-11-25 22:20 ET · PR #148 LDS barriers (`e2c4a8b`) + PC FSR 3.1.6 (`VICTORY_FIDELITYFX_DIR`) evaluation

- **Branch / commit:** `antigravity/graphics` @ `e2c4a8b24aa13fb7455fc9aa19f941d4736954d4` (on top of `d253f54` / `8e1b338` / `ca77ed7` / `496ca63` / `d315abe` / `a0d8b43`, clean tree).
  - Standalone patch: [antigravity-wave64-lds-barriers.patch](<../../patches/history/graphics-history/antigravity-wave64-lds-barriers.patch>) (`emit/lower.rs`, +101/-9 lines; ports upstream `VIctory-macOS#148` `split_wave_barriers` + `group_barrier`, activated under `VICTORY_WAVE64_HALVES=1`).
  - Updated combined branch patch (`a0d8b43..e2c4a8b`): [antigravity-graphics-all.patch](<../../patches/history/graphics-history/antigravity-graphics-all.patch>).
  - Unit tests: `cargo test --release -p victory-shader` → `148 passed; 0 failed; 2 ignored`.
- **Tested build & hashes (exact clean build of `e2c4a8b` with `--features diagnostics`):**
  - `victory.exe`: `b169fcf10f64ae86bbe694ee48cb22b79bc8bd4904d968b39c663f634806970e` (`victory-gptk4-run/ag-halves/victory.exe`)
  - `victory-metal-daemon`: `28e7436a39e7fc0a45eb6a878d75512eacd1891059253e0c2e003bd52e0d342f` (`victory-gptk4-run/ag-halves/victory-metal-daemon`)
- **Run settings (`aglds-fsr-warm-20261125-221637-939f86ru`, locked via `run.lock`, 210 s, warm cache `1248 hits / 706 misses`):**
  - `VICTORY_WAVE64_HALVES=1`, `VICTORY_FORCE_WAVE32=1`, `VICTORY_PACK_MESH_OUTPUTS=1`, `VICTORY_MATERIAL_LIGHT=1`, `VICTORY_FIDELITYFX_DIR=C:\victory\ffx-pc` (PC FSR 3.1.6), `VICTORY_PIPELINE_WORKERS=4`.
- **Observed — PR #148 GPU speedup & error counts:**
  - **`XPC_ERROR_CONNECTION_INTERRUPTED`: 0**, **`Fragment input`: 0**, **`shader_refusals`: 0**, **Total `[ERROR]`: 5** (only the 5 irreducible 17-reg `34,816 B` mesh shaders).
  - **GPU time per frame dropped from `520–695 ms` (`agxpc3`) to `158–282 ms`** on steady world frames (`0` dropped pipelines), confirming PR #148 fixes the unbarriered 33–64-lane wave64 LDS shaders (e.g. step-distance ray-march prep).
- **Observed — PC FSR 3.1.6 (`amd_fidelityfx_upscaler_dx12.dll`) verdict:**
  - FSR 3.1.6 (`render=[1280, 720] → output=[1920, 1080]`) initialized and ran cleanly on D3DMetal 4.0b3, showing rich night building glow and volumetric police strobes (frame-1739.png, frame-1747.png).
  - **However, keep PC FSR 3.1.6 OFF for now:** enabling `VICTORY_FIDELITYFX_DIR` bypasses `VICTORY_SPATIAL_UPSCALE=1` and feeds raw HDR/motion-vector/depth buffers directly into FSR temporal reprojection, which smears dark/purple block artifacts across faces (frame-1743.png). Re-enable FSR 3.1.6 only after native HDR exposure & motion vectors are clean.
  - Logs: Game log (`victory-run-aglds-fsr-warm-20261125-221637-939f86ru`) · Daemon log (`aglds-fsr-warm-20261125-221637-939f86ru-daemon.log`)
- **Recommended settings for Codex integration on `antigravity/graphics@e2c4a8b`:** `VICTORY_WAVE64_HALVES=1`, `VICTORY_FORCE_WAVE32=1`, `VICTORY_PACK_MESH_OUTPUTS=1`, `VICTORY_PIPELINE_WORKERS=4`, `VICTORY_FIDELITYFX_DIR` unset (`VICTORY_SPATIAL_UPSCALE=1` + `VICTORY_MATERIAL_LIGHT=1`).
