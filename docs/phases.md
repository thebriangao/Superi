# Superi: Project Plan & Phase Structure

The canonical phase structure for Superi, organized by **build sequence** (how the work and teams are sequenced), with the **capability progression** (what Superi can do over time) preserved inside it. This is both a working-memory reference and a founder-orientation map.

**Locked foundational stack:** Rust engine · wgpu graphics · Rust-native (OTIO-compatible) timeline model · full MIT.

---

## ⚠️ The one principle that shapes everything: integration is continuous, not a later phase

The single most important lesson baked into this plan: **you do not build all the parts in isolation and then integrate them in a separate phase.** Doing that creates an "integration cliff", the moment you finally wire finished components together, every untested assumption about how they fit surfaces at once, producing massive, demoralizing rework where parts that were each "done" turn out not to mesh.

Instead, **a thin vertical slice runs through the components as they are built**, so each part's shape is validated by something consuming it the moment it exists. The orchestration work (Phase 2) is therefore the *deepening and hardening* of integration that has been happening all along, not the first attempt at it.

This principle recurs at every level: the substrate is built pulled by the slice; the public API is shaped by a real consumer; the parts and their integration are not sequential. **If you remember one thing from this plan, remember this.**

---

# THE PHASES (canonical, build-sequence framing)

## Phase 0: Planning & Decisions *(now)*

**What it is:** Orientation and the irreversible decisions. Mapping the system, locking choices, producing the artifacts that let you hire well.

**Work:**
- Lock irreversible decisions ✅ (Rust, wgpu, full MIT, Rust-native OTIO-compatible timeline model)
- Architecture document (the artifact handed to candidate engineers)
- Licensing/IP strategy (the MIT-clean / encumbered-codec boundary, lawyer-reviewed)
- Define the vertical slice that will thread through Phase 1

**Exit condition:** Decisions locked and written architecture exists, enabling credible hiring.

**Team:** Founder + (toward end) a small senior founding team of 5-8 specialists, heavy on graphics/codec/audio. Not more.

---

## Phase 1: Build the Engine Parts *(heavy engine code, built WITH continuous integration)*

**What it is:** Building all the underlying modules/dependencies, the Rust-native substrate replacing the lost C++/ASWF ecosystem. This is the bulk of the hard engine work.

**Built in dependency order, pulled by the vertical slice** (import → single-track timeline → trim → one effect → export) so each part is validated by use as it's made. The slice is the continuous-integration harness, NOT a feature rush.

**The parts** (full detail in the Subsystem Inventory below):
- Media I/O & codecs (decode/encode behind a swappable boundary)
- GPU pipeline on wgpu (GPU-resident frames)
- Color substrate (linear 16-bit float; OCIO replacement)
- Image primitives (OIIO/OpenEXR replacement)
- Node-graph engine + evaluator (the core)
- Caching & proxy/optimized media
- Concurrency & scheduling (threading model)
- Timeline/editorial data model
- Audio engine (its own subsystem)
- Effects & extensibility conventions
- AI inference layer

**Exit condition:** Every required subsystem exists and the thin slice runs end-to-end through all of them, real-time GPU playback of 4K through the actual graph engine.

---

## Phase 2: Orchestration & Integration *(engine code, the culmination of integration)*

**What it is:** Hardening the continuous integration from Phase 1 into a genuinely coherent, performant, complete engine. **This is real, distinct, foundational engine work, and it is NOT UI work.** It is some of the hardest, most bug-prone code in the project, because it only exists by binding the other parts together.

**NOTE:** This is the *deepening* of integration, not its start. If Phase 1 was done in isolation, this phase becomes the integration cliff. If Phase 1 was done with the slice threading through it (as intended), this phase is the natural hardening into full orchestration.

**The work** (full detail in Section 13 of the Inventory):
- Playback orchestration (decoder + graph + cache + audio + clock → synchronized real-time output)
- Render/export orchestration (the end-to-end decode→graph→color→encode flow)
- A/V sync coordination under real conditions
- Engine-wide command/transaction model (undo, persistence, API all route through it)
- **The unified public engine API**, the single surface the UI is later built against
- Subsystem lifecycle & shared-state management
- Cross-subsystem error propagation & recovery
- Cross-subsystem memory/resource arbitration
- Integration validation in concert under real conditions

**Exit condition:** A running, integrated, headless engine driven entirely through its public API, proven by the slice/CLI exercising that API throughout.

---

## Phase 3: The UI / The Actual Editor

**What it is:** Building the visual editor on top of the running engine, making it look good and fully functional. **The UI team builds against the engine's public API and never writes the underlying edit logic themselves.**

**Carry-forward caution:** The API the UI builds against should already have been exercised by the slice/CLI throughout Phases 1-2, so it's a proven shape rather than an untested guess. (Same continuous-integration principle, one more time.)

**This phase is where the CAPABILITY PROGRESSION plays out**, Superi grows from a cutting tool into the full vision, one discipline at a time, each additive because the node graph was built first:

- **3a, v1, the Professional Timeline Editor** *(first public-quality milestone)*: usable timeline NLE, proper editing ops (ripple/roll/slip/slide/razor/3-4-point/snapping/markers), proxy workflow, foundational color (primary + scopes), real multi-track audio, reliable export, focused AI set (transcription/captions, silence detection, scene detection, denoise). *Bar: a working editor would choose Superi for a real cut and find it solid.*
- **3b, Compositing & Motion (After Effects axis)**: layered/graph compositing, keyframed effects, masking/rotoscoping, text & motion design, OFX plugin interface.
- **3c, Advanced Color (Resolve axis)**: node-based secondary grading, full color page, HDR, advanced scopes.
- **3d, Pro Audio Maturity & Deeper AI**: VST/AU hosting & real mixing; AI expansion (roto assist, shot matching, generative), all still producing editable graph artifacts.
- **3e, Unification**: the three disciplines feel like one environment, not three modes.

*(These sub-stages may overlap with later phases; capability growth is additive and ongoing, not a single hand-off.)*

**Exit condition:** A fully functional, good-looking editor covering (at minimum) the v1 editing lane, with further disciplines layered on.

---

## Phase 4: Private/Beta Testing, Optimization & Finalization

**What it is:** Hardening against real users, real footage, and real edge cases; squeezing performance; finalizing for release.

**Important framing:** This is NOT where integration testing first happens (that's been continuous since Phase 1). This is where you harden against the messy real world. Optimization here is real, but the *foundational* performance properties, stutter-free playback, GPU-residency, were architected in from the start, because they cannot be bolted on late.

**Work:** private/beta program, real-world edge-case hardening, performance optimization, stability, polish, finalization.

**Exit condition:** Stable, performant, ready for open public use.

---

## Phase 5: Public Open-Source Launch

**What it is:** The MIT, fully open-source public release.

**Why the earlier decisions pay off here:** the thing you launch openly only holds together as a genuine open-source release because the MIT-clean / encumbered-codec boundary was respected all the way back in the engine work. This is also the start of the asymptotic, never-"finished" life of the project, community, ecosystem, continuous approach toward the north star.

**Exit condition:** Public MIT launch. (And then: it never fully "completes", ongoing growth toward the end goal.)

---

# Two phase lenses: how to read them

This plan deliberately carries two views of the same project:

1. **Build-sequence (canonical, the numbered phases above):** plan → build parts → orchestrate → UI → test → launch. This is how the *work and teams* are sequenced, and the language you'll use with engineers.
2. **Capability progression (nested inside Phase 3):** engine core → v1 editor → compositing → color → audio/AI → unification. This is *what Superi can do* over time, the roadmap toward the end goal.

When someone says "Phase 2," it means **orchestration** in the canonical scheme. Use the numbered phases as the shared vocabulary.

---

# END GOAL (the north star)

A **unified, fully open-source (MIT) post-production environment** where editing (Premiere-level), compositing & motion (After Effects-level), and color (Resolve-level) share one node-graph engine, backed by a professional audio subsystem, with AI woven ambiently throughout, every AI output an editable graph artifact, never a black box. The encumbered codec layer stays cleanly separated so the core remains MIT-clean.

**Honest framing:** feature parity with three tools that have a combined 70+ years of development is *asymptotic*, you approach it, you don't tick it off. The real target: **the open-source tool a working professional can genuinely live in for the majority of real projects.** That, a flagship-quality editor that's actually free and open, is the thing that has never existed, and the thing worth the funding and the years.

---
---

# Superi: Skeletal Dependency & Subsystem Inventory

> **Purpose of this section:** A complete map of the *underlying technology* (the bones, zero UI) that the eventual Superi must contain. This is a scope-creep guard and founder orientation tool, **not** a build order, sequencing lives in the phases above. Most items below are Rust-native rewrites/ports of capabilities the lost C++/ASWF ecosystem (OCIO, OIIO, OpenEXR, OFX, OTIO) would have provided. An item appearing here does **not** mean "build now"; it means "must not be forgotten when building the UI on top."
>
> Items marked **[FND]** are foundation-critical (cannot be retrofitted; shape everything above). Items marked **[ADD]** are additive subsystems layered onto the proven core.

## 1. Media I/O & Codecs
- **[FND]** Decode interface, abstract "give me frame N of source" boundary (codec-agnostic).
- **[FND]** Encode interface, abstract "take these frames → deliverable file" boundary.
- **[FND]** Concrete decode/encode module behind the interface (the swappable, encumbered-isolated layer).
- Demuxing / container parsing (MOV, MP4, MXF, etc.).
- Timestamp / timecode handling and frame-accurate seeking.
- Variable frame rate & drop-frame timecode handling.
- Audio stream demux/decode/encode (paired with video).
- Image-sequence I/O (EXR/DPX/PNG sequences as sources and outputs).
- RAW / camera-format handling (long-tail, additive).

## 2. GPU Pipeline (wgpu)
- **[FND]** GPU buffer & texture management / allocation.
- **[FND]** Decoded-frame upload path (CPU → GPU), keeping frames GPU-resident.
- **[FND]** Pixel format conversions (YUV families → working textures and back).
- **[FND]** Shader infrastructure / compute & render pass orchestration.
- GPU memory pooling & pressure management for long sessions.
- Multi-GPU / device selection (additive).
- Readback path (GPU → CPU) for export and thumbnails only.

## 3. Color Substrate (OCIO replacement: heavy rewrite)
- **[FND]** Linear, 16-bit-float internal working space.
- **[FND]** Color-space transforms in (footage native → linear working).
- **[FND]** Color-space transforms out (working → display / delivery).
- **[FND]** Configurable color-management config system (OCIO-config-equivalent).
- Display / view transforms & viewing rules.
- HDR transfer functions & wide-gamut handling.
- LUT loading & application (1D/3D).
- ICC / display profile awareness.

## 4. Image Primitives (OIIO / OpenEXR replacement)
- **[FND]** High-dynamic-range, high-bit-depth image data model (EXR-equivalent).
- **[FND]** Core pixel operations (resize, crop, transform, blend, composite ops).
- Wide format read/write breadth (the OIIO-style universal image layer).
- Tiled / scanline / mipmap handling for large images.
- Channel & layer model (multi-channel/multi-layer images).
- Image metadata model.

## 5. Node-Graph Engine (the core)
- **[FND]** Directed acyclic graph data structure (nodes = GPU ops, edges = pixel flow).
- **[FND]** Lazy, per-frame, per-region evaluator.
- **[FND]** Node input/output contract & type system (what flows between nodes).
- **[FND]** Graph mutation API (the surface the timeline & UI compile down to).
- Graph serialization / deserialization (project persistence at engine level).
- Region-of-interest / dirty-region propagation.
- Deterministic, headless evaluation (CLI/CI render parity with UI).
- Expression / parameter-linking system (drives keyframing & rigging later).

## 6. Caching & Media Optimization
- **[FND]** Frame cache (final frames + intermediate node outputs).
- **[FND]** Proxy / optimized-media generation & substitution.
- Render cache / background-render system.
- Prefetch & predictive caching for playback.
- Cache eviction & memory-budget policy.
- On-disk persistent cache.

## 7. Concurrency & Scheduling
- **[FND]** Render/playback paths separated from UI thread.
- **[FND]** Job system for parallel frame/tile work across cores.
- **[FND]** GPU command submission & synchronization model.
- Playback clock & A/V sync scheduler.
- Shared-state model (cache, buffers, clock) leveraging Rust's Send/Sync guarantees.

## 8. Timeline / Editorial Data Model
- **[FND]** Rust-native timeline model (tracks, clips, transitions, edit decisions).
- **[FND]** OTIO-compatible serialization (import/export interchange, *do not strand on proprietary format*).
- Edit-operation primitives at data level (ripple, roll, slip, slide, razor, 3/4-point).
- Multicam data model.
- Markers, metadata, bins/media-management model.
- Nested sequences / compound clips.

## 9. Audio Engine (its own subsystem, not a feature)
- **[FND]** Separate audio processing graph.
- **[FND]** Sample-accurate A/V sync.
- **[FND]** Low-latency playback path.
- **[FND]** Mixing architecture (buses, levels, fades).
- Sample-rate conversion & resampling.
- Audio metering & analysis.
- **[ADD]** VST3 / AU plugin hosting.
- **[ADD]** Advanced mixing / routing / automation.

## 10. Effects & Extensibility
- **[FND]** Internal effect/node authoring conventions (how a node is defined).
- **[FND]** Keyframing & animation system (parameters over time).
- Masking / roto data model & rendering.
- Transitions framework.
- **[ADD]** OFX-compatible plugin interface (inherit existing effects ecosystem).
- **[ADD]** Text & motion-design primitives.
- **[ADD]** Tracking / motion-tracking data + solver.

## 11. Scattered AI (open tier: local, offline, bundled, MIT)
> **Open-tier AI only.** Every item runs on a bundled, permissively-licensed model entirely offline (must survive the unplugged-cable test) and transforms content the user *already has*, never generates new content. The proprietary Superi Max tier (media generation + agent) is **not** listed here: its generation models are third-party services that exist out in the world (not bones we build), and its own buildable bones, agent orchestration, account/credit infrastructure, permission model, editor-control integration, belong to the **separate proprietary codebase across the boundary**, not to this open-tier inventory.
- **[FND]** Local inference runtime, runs bundled, permissively-licensed models on-device, **offline only** (no remote path; a remote path would breach the offline boundary).
- **[FND]** AI outputs as standard editable graph artifacts (mask nodes, color ops, edit decisions), never black-box bypass of the graph.
- **[FND]** Bundled-model licensing audit hook, every shipped model verified permissive/redistributable.
- Auto-captioning / transcription (captions & subtitles from existing dialogue).
- Audio denoising (remove hiss, hum, background noise).
- Silence detection & removal (trim dead air and long pauses).
- Filler-word detection (flag/remove "um," "uh," "like").
- Speaker diarization (auto-label who is speaking when).
- Background removal / subject masking (isolate a subject from its background).
- Auto-reframe (re-crop horizontal → vertical/square, keeping the subject framed).
- Scene / cut detection (split a long clip at shot changes).
- Object & face tracking (track a subject to drive masks and effects).
- Auto color matching (match two existing shots for consistency).
- Content-based media search & tagging (search own footage by content, no manual labeling).
- Transcript-based editing (edit the video by editing its transcript, closest to the agent seam; kept scattered for being a single deterministic transcript→cut mapping).

## 12. Cross-Cutting Engine Concerns
- Project / document model & persistence (serialize whole engine state).
- Undo/redo model (engine-level command history).
- Render/export pipeline & queue (stitches decode→graph→encode for delivery).
- Logging, diagnostics, profiling instrumentation.
- Plugin/extension loading & sandboxing model.
- Scripting / automation API surface (also the engine's public API).
- Settings / configuration system.
- Crash recovery / autosave at engine level.
- Color/format/codec capability introspection (what the build can actually open & deliver).

## 13. Orchestration & Integration Layer (engine code, NOT UI)
> **This is the connective tissue that binds the subsystems above into a single coherent system.** It does not belong to any one component, and it is emphatically *not* UI work, it is some of the hardest, most bug-prone engine code in the whole project, and it only exists by wiring the other bones together. A pile of individually-finished components is not a system until this layer is built. **All [FND] here must be done by the engine team before the UI team can build on a running engine.** (This is Phase 2 in the canonical scheme.)
- **[FND]** Playback orchestration, coordinates decoder + graph evaluator + cache + audio engine + clock into synchronized real-time output.
- **[FND]** Render/export orchestration, the actual end-to-end flow binding decode → graph → color → encode into one delivery operation (distinct from the export *queue* line item above; this is the flow itself).
- **[FND]** A/V synchronization coordination across the video and audio subsystems under real playback conditions.
- **[FND]** Engine-wide command/transaction model that undo/redo, persistence, and the API all route through (so an edit is one coherent operation across every affected subsystem).
- **[FND]** The unified public engine API, the single surface through which *anything* (UI or AI) drives the engine; the contract the UI is later built against.
- **[FND]** Subsystem lifecycle & state management, initialization, teardown, and shared-state coordination (cache, buffers, clock, devices) across all subsystems.
- **[FND]** Cross-subsystem error propagation & recovery (a failure in decode/GPU/audio surfaces coherently rather than corrupting or deadlocking the whole).
- **[FND]** Memory-pressure & resource arbitration *across* subsystems (cache vs. GPU buffers vs. decode buffers competing for the same finite budget).
- **[FND]** Integration validation, proving the subsystems work *in concert* under real conditions (format mismatches, sync races, color round-trips, memory interactions), engine-level work that happens *after* each component is individually "done."

---

### Notes on using this inventory
- **[FND] items are the "minimal complete substrate"**, the bones the end-to-end vertical thread must exercise to run. They are built *pulled by the slice*, in dependency order, not all at once in isolation.
- **[ADD] items are real but layered on a proven core**, listed here only so they aren't forgotten, not so they're built early.
- **Section 13 is the bridge between "all components built" and "UI assembles the editor."** It is engine work, not assembly. Until it exists, there is no integrated engine for a UI to sit on, only correct parts that don't yet hold together.
- The discipline this section enforces: when building the UI, every user-facing affordance should map to a bone already listed here. If it doesn't, either the bone is missing (add it deliberately) or the feature is scope creep (cut it).

### On sequencing the engine team before the UI team
- **The clean split you want is real and largely achievable:** the engine team builds all [FND] substance *and* the Section 13 orchestration layer first (Phases 1-2), producing a running, integrated, headless engine driven entirely through its public API. Only then does the UI team build the visual editor on top (Phase 3). The UI team genuinely never writes the underlying edit logic.
- **One honest caveat:** a fully serial hand-off ("engine 100% done, *then* hire UI, *then* start UI") is cleaner on paper than in practice. The public engine API is best shaped *with* a consumer pulling on it, the same reason the substrate is best built pulled by the vertical slice.
- **The refinement that keeps your clean split while avoiding that trap:** let the headless vertical slice and a thin scripting/CLI harness act as the engine's "first consumer" throughout Phases 1-2, so the API is validated by real use before any UI exists. The engine→UI hand-off is then to a proven, exercised API. You still get a clean two-phase structure and avoid running two large teams concurrently; the engine is simply *consumed* (by the slice/CLI) as it's built, not just written.