# Platform expansion (R20): research

This covers roadmap item R20, section 8 (Phase 5) of [competitive-development-roadmap.md](competitive-development-roadmap.md). It is **research only**. The [development policy](../development-policy.md) puts desktop first and defers mobile and web until the desktop engine is reliable, so this document builds nothing. It records what each target would need from the engine as it is today, what blocks it, and the smallest first step for when Paul decides to go.

Claims about third-party crates are from their documentation as of late 2026 and are marked **[check]** where a spike must confirm them before any work starts.

## 1. Summary

| Target | Verdict | Biggest blockers | First step when allowed |
|---|---|---|---|
| **Android** (8.1, first mobile target) | Feasible later; the most practical non-desktop target | Texture formats (BC7 only), touch UI and input, a mobile render profile, Luau JIT on ARM, packaging and signing | A player-only APK that opens `hello-part` with ASTC textures and the Low preset |
| **iOS** | After Android | The same as Android, plus no JIT (App Store rules), Metal only, and Apple signing and review | Only once Android ships |
| **Web** (8.2) | Not yet; would duplicate too much today | No compute fallback for several passes on WebGL 2, Luau on wasm, UDP networking, threads, asset size | A WebGPU-only player spike, after the runtime can run single-threaded |
| **VR** (8.3) | Feasible as a desktop add-on (PC VR through OpenXR) | Stereo rendering, frame pacing at 90 Hz, controller input, comfort tools | An OpenXR spike: the player renders `hello-part` to a headset at 90 Hz |
| **AR** | Not planned | Needs mobile first | — |
| **Consoles** (8.4) | Not possible without platform programs | NDAs, dev kits, certification; no public toolchains | None until a platform program accepts Axcenity |
| **Advanced simulation** (8.5) | Per feature, when a game needs it | — | See section 7 |

## 2. What the engine already has that helps

- **wgpu 30.** One renderer over Vulkan, Metal, DX12, GL and WebGPU. Android (Vulkan or GLES), iOS (Metal) and browsers (WebGPU, WebGL 2) are wgpu backends **[check per feature]**.
- **`Capabilities` and graphics settings.** `render::Capabilities` (compute shaders, texture limits, sample counts, ray queries) and `GraphicsSettings::effective` already turn off what a GPU can't do and leave a note. For example, real-time GI and volumetric fog need compute shaders (`render-types/src/graphics.rs`). A mobile profile would be another preset, not another renderer.
- **Cooked textures** (L2, [textures.md](../rendering/textures.md)). `axcenity-cook` BC7-encodes textures into `.axtex` files, and import presets (R12) choose how each is cooked. A target-specific format (ASTC, ETC2) would be another cook output chosen by the build profile, not an engine change.
- **winit 0.30** supports Android (through `android-activity`), iOS and the web **[check]**.
- **Pure-Rust physics and audio.** rapier3d (vendored) builds for wasm and ARM. kira plays through cpal, which has Android (AAudio/OpenSL), iOS (CoreAudio) and Web Audio backends **[check]**.
- **Build profiles (R13)** already choose what a build contains. A target would be one more profile field.
- **The player is separate from the editor.** Only `player` (and `sim`, `script`, `render`, `audio`, `physics`) would need to run on a new target. The editor stays on desktop.

## 3. Mobile (8.1)

| Item | Today | Needed |
|---|---|---|
| Rendering backend | wgpu, tuned on desktop GPUs | Vulkan 1.1 on Android (GLES 3 fallback for old devices **[check]**), Metal on iOS. Tile-based GPUs dislike extra full-screen passes: a mobile preset with no SSAO, no volumetrics, no TAA history at full size, at most one shadow cascade, and the forward pass with MSAA ×4 instead of post AA |
| GPU tiers | Presets Low to Ultra, capability notes | A `Mobile` preset under Low, picked from the adapter's limits and vendor at first start. Read `Capabilities` as today |
| Textures | BC1–BC7 cooked (desktop wins, policy 3) | ASTC 4×4/6×6 (and ETC2 for GLES) as a cook target per build profile. wgpu needs `TEXTURE_COMPRESSION_ASTC` |
| Scripting | Luau with codegen (`luau-jit`) | Android ARM64: Luau's codegen supports A64 **[check]**. iOS: no JIT allowed, so the interpreter only (the `luau` feature without `luau-jit`). This is fine for most game scripts |
| Input | Keyboard, mouse, gamepad (gilrs), input map actions | Touch: `InputObject`s with `UserInputType.Touch`, the input map's actions bound to on-screen controls (thumbstick, jump and action buttons) drawn by `ui` and hidden on desktop. `UserInputService.TouchEnabled` |
| Touch UI | ScreenGui with UDim2, scale-aware | Safe areas (notches), a minimum touch size, and font scaling. Mostly settings on the existing UI |
| Performance profiles | Benchmarks on the 9070 XT and a Mac | One mid-range Android phone as the reference device, with `bench_*` runs over adb |
| Packaging and signing | Installers per desktop OS (`packaging/`) | Android: `cargo-apk` or `xbuild` **[check]**, an AAB for the Play Store, a keystore kept out of the project (as game keys in R19). iOS: an Xcode project wrapper, an Apple Developer account, provisioning |
| Lifecycle | Desktop window | Suspend and resume (the GPU surface is lost on Android pause), and saves on pause |

**Android first**, as section 8.1 asks. It allows JIT, sideloading for testing, and Vulkan, and a Windows or Linux PC can build it.

## 4. Web (8.2)

| Item | Today | Problem on the web |
|---|---|---|
| Graphics | Compute passes for GPU particles, real-time GI, volumetric fog, ray tracing, some post | WebGPU has compute. WebGL 2 has none, so a WebGL 2 build would need the CPU fallbacks every compute pass already has (particles, fog off, GI off) **[check each pass]**. Section 8.2 says no export "without excessive duplication", so the target would be **WebGPU only** |
| Scripting | mlua with Luau | Luau compiles to wasm through Emscripten. mlua's support for `wasm32-unknown-unknown` is limited **[check]**. A spike must show a Luau VM running in the browser player, with no JIT |
| Threads | Asset loading, CSG, audio and baking use threads | Browser threads need cross-origin isolation headers and SharedArrayBuffer. The simplest route is a single-threaded runtime with async asset loading **[check]** |
| Networking | renet over UDP | Browsers have no UDP. It would need a WebTransport (or WebSocket) transport for renet, and the server listening on both |
| Audio | kira on cpal | Web Audio through cpal works but must start after a user gesture |
| Assets | A packed `.axgame`, cooked textures | Streaming over HTTP (R16's partition fits), browser memory caps (about 2–4 GB for wasm32), and BC7 in browsers only on desktop GPUs. That suits a web build aimed at desktop browsers |
| Packaging | Installers | A static folder: `index.html`, wasm, packed assets, and headers for COOP/COEP |

Verdict: wait until (a) the runtime can run without its own threads, (b) Luau runs in a browser in a spike, and (c) WebGPU ships by default in Chrome, Edge, Firefox and Safari **[check]**. A web build then shares everything but the transport and the loop.

## 5. VR and AR (8.3)

- **OpenXR** through the `openxr` crate on desktop (SteamVR, Meta Link, WMR) **[check its wgpu interop on Vulkan and DX12]**. wgpu needs the swapchain images that OpenXR creates. This works through wgpu-hal interop and is the main technical risk.
- **Stereo rendering:** two views of the same draw list. The renderer already draws several views (editor viewports, probes), so start with two passes and add multiview later if measured.
- **Frame pacing:** 90 Hz means 11.1 ms for both eyes. The 9070 XT numbers (S10k at 5.14 ms) suggest High fits for typical scenes. A VR preset turns off TAA history (reprojection does that job) and motion blur.
- **Input:** controllers as `InputObject`s with poses, and the input map's actions bound to XR actions. Hand tracking comes later.
- **Interaction tools:** grab, teleport and snap-turn as `gameplay` systems, like Tools and Abilities, so games get comfort options without scripts.
- **Spatial audio:** kira already places sounds in 3D. HRTF would be a later quality step.
- **AR** needs mobile (ARCore/ARKit) or a passthrough headset. Revisit after Android.

## 6. Consoles (8.4)

Console SDKs, compilers and dev kits are under NDA and only open to accepted developers. Rust and wgpu have no public console backends. Certification adds platform-specific rules (save data, suspend, accounts, achievements). Nothing can be researched further in public. The engine's part, once a program accepts it, would be a wgpu or HAL backend kept private per platform, a platform layer for input, saves and accounts, and a separate cook target. **No work until then.**

## 7. Advanced simulation (8.5)

These are per feature, and only when a real game needs them (policy: fix correctness before features).

| Feature | Fit | When |
|---|---|---|
| Cloth | A GPU or CPU position-based dynamics solver on a skinned mesh's vertices, colliding with capsules from the rig | When a game's characters need capes or flags. A vertex-shader wind sway covers foliage already |
| Soft bodies | Rapier has no soft bodies; PBD as for cloth | Same as cloth |
| Hair | Strands are heavy; cards with physics on their roots are what games ship | After cloth |
| Motion matching | Needs a large mocap database per game; deferred (section 9) | Deferred |
| Active ragdolls | Ragdolls exist (`Ragdoll`); active ones add joint motors toward the animated pose, which rapier supports | The cheapest of these; a candidate if a game wants hit reactions |
| Procedural animation | IK and retargeting exist (G16) | As games ask |

## 8. Order, if Paul says go

1. **Android spike (P1):** the player with Vulkan, ASTC cooking, touch input and the `Mobile` preset, on one reference phone. Measure frame time and memory.
2. **OpenXR spike (P2):** stereo `hello-part` at 90 Hz on PC VR. It needs no mobile work, and it uses the 9070 XT.
3. **Android productization:** packaging, signing, lifecycle, on-screen controls, and a build profile.
4. **Web spike:** a WebGPU player, single-threaded, after the runtime work in section 4.
5. **iOS**, after Android ships.

Each spike is one PR with a document of its measurements. None starts before Paul lifts the policy's deferral for that target.
