# Superi: Engineering & Architecture Document

**Status:** Foundational. Living document, versioned and dated, expected to evolve as the vertical slice is built and the founding engineers pressure-test its assumptions.
**Version:** 0.2 (Phase 0 contracts ratified)
**Audience:** Founding engineers, candidate engineers, technical leadership. This document captures *decisions with their rationale and rejected alternatives*, at the level of how subsystems are structured and how they connect, not implementation prescription, which is the specialists' to determine.

---

## 0. How to read this document

This is the consolidation of Superi's foundational thinking. It defines the destination (the North Star), the hard product/licensing boundaries that shape the architecture, the locked technology stack and the reasoning behind each choice, the subsystem inventory (the "bones"), the orchestration that binds them, the build phasing, and the brand direction. It also honestly marks what is **locked** versus what remains a **founding-team decision**.

The detailed, ratified implementation contracts for the desktop boundary, public API transport,
native viewport, OTIO interchange, project format, color model, threading, plugins, licensing, and
measurable completion targets live in [`phase-0-build-contracts.md`](phase-0-build-contracts.md).

The test every downstream decision must pass: *does this serve the North Star, nothing more, nothing less?*

---

## 1. North Star: the destination

Superi is a professional post-production environment delivered in **two tiers** divided by one hard, physical boundary:

- **Superi (open, MIT)**, a complete, professional editor that runs entirely on the user's own machine, plus a set of local AI conveniences. Free, open-source, forkable, and **fully functional with the network physically unplugged**, no account, no servers, no credits, no degradation.
- **Superi Max (closed, proprietary)**, a server-backed, account-gated, credit-metered layer that attaches to the open editor: manual media generation and an orchestrating agent. This is the commercial product and the business.

The relationship is **VSCode-to-Cursor**: the open editor is genuinely complete and powerful on its own; the proprietary layer adds server-dependent intelligence on top. The open tier drives adoption and community; the closed tier drives revenue.

**The honest framing of "done":** matching three flagship tools (Resolve, Premiere, After Effects) that each carry decades of development is **asymptotic**, Superi approaches the bar continuously rather than ticking it off. The North Star is defined not as feature-for-feature parity, but as **the threshold at which a working professional can genuinely live in Superi for the majority of real projects** and choose it on its merits, with its openness and the create-the-nonexistent power of Superi Max as decisive advantages.

**The thing that has never existed:** a flagship-quality post-production environment that is genuinely free and open and works entirely offline, with a proprietary generation-and-agent layer on top for those who want it.

---

## 2. The hard boundary (non-negotiable, and an architectural constraint)

This is the most important rule in the system. It is a **licensing** boundary, a **product** boundary, and, critically, an **architectural** boundary.

### The test
**Unplug the network, and the open editor must work end to end**, all editing, all local AI, full project save/load, no account, no degradation. If it works, the boundary holds; if anything reaches for a connection, that is a breach.

### Enforcement
- A **network-isolated CI test** that builds the open tree in a sandbox and runs a full editing workflow, failing the build if anything reaches for a connection. This makes the boundary self-policing and impossible to erode silently.
- A **bundled-component license audit**: every model and dependency shipped in the open tree is verified genuinely permissive/redistributable. (This is the *sibling guarantee* to the offline test, the cable proves independence from our infrastructure; the audit proves we have the right to give away everything in the box. They are different guarantees and both are required.)

### The cardinal rule
**The open tree never imports, links, or depends on the closed tree, and must build and run completely without it.** Direction of dependence is one-way: the closed tier depends on the open editor's public API; never the reverse.

### The governing principle (also Superi's core value insight)
**Transform what already exists → open and local. Generate what never existed → closed and server-backed.**

This single line governs what goes where. It is drawn **on principle, not on what is momentarily possible locally**, so it stays legible even as local models improve. (If we let the line drift every time local models get better, the boundary becomes incoherent and users can't predict what's free. Anchoring it to the durable conceptual distinction keeps it a principled commitment to our open community.)

---

## 3. Locked technology stack & the reasoning

Each choice below is **locked as a directional decision**. Detailed Phase 0 contracts and their
change-control requirements live in [`phase-0-build-contracts.md`](phase-0-build-contracts.md).

### 3.1 Engine language: Rust *(locked)*
**Decision:** The engine is written in Rust.
**Rationale:** Superi is a long-running, buffer-heavy (gigabytes of frame data held for hours), heavily-threaded, crash-intolerant systems application. Rust's compile-time **memory safety** eliminates the dominant catastrophic-bug classes (use-after-free, dangling pointers, buffer overruns) before they reach a user, and its **fearless concurrency** makes data races a compile error, both mapping almost perfectly onto Superi's specific risk profile.
**Rejected alternative, C++:** Genuinely competitive, and rejected only after real consideration. C++'s decisive advantage was the **ASWF film-tooling ecosystem** (OpenColorIO, OpenImageIO, OpenEXR, OpenFX, OpenTimelineIO), decades of battle-tested, C++-native libraries Superi could have linked directly. Choosing Rust means that substrate must be **rewritten/re-bound in Rust**, which is a real, accepted cost (see the Inventory, much of the color/image/effects substrate is exactly this rebuild). The bet: long-term robustness of a memory-safe, modern-tooled foundation outweighs the upfront cost of rebuilding the film libraries.
**Accepted costs:** slower early velocity (the borrow checker that prevents bugs also slows engineers until fluent); a narrower talent pool of deep-Rust-plus-graphics engineers (weight founding hires accordingly); and the reality that GPU and codec boundaries are `unsafe` FFI territory where the guarantees thin out, but those boundaries are contained behind clean interfaces.

### 3.2 Graphics: wgpu *(locked)*
**Decision:** GPU work goes through wgpu.
**Rationale:** wgpu is the Rust-native GPU abstraction, targeting Vulkan, Metal, and D3D12 underneath, so Superi writes its GPU-heavy core **once** and runs natively across Mac, Windows, and Linux. This solves cross-platform reach *and* native-Metal performance (critical, since much professional editing is on macOS) in a single decision, instead of hand-rolling an abstraction over raw Vulkan. It also fits the Rust engine cleanly rather than bolting a foreign graphics system onto a Rust codebase.
**Accepted cost:** wgpu is younger than raw Vulkan tooling; budget for occasional rough edges at the GPU boundary (which is `unsafe` FFI territory regardless).

### 3.3 Timeline data model: Rust-native, OTIO-compatible *(locked)*
**Decision:** A Rust-native timeline/editorial data model that is **OpenTimelineIO-compatible**, serializable to and from the OTIO schema even though the in-memory representation is our own.
**Rationale:** A Rust-native model keeps the engine coherent and avoids permanently maintaining bindings to C++ OTIO. But OTIO's real value was never its code, it was **industry interchange** (round-tripping edits with the rest of the professional toolchain). Stranding Superi on a proprietary format would directly undercut "professionals can actually adopt this." So we faithfully recreate OTIO-schema compatibility at the import/export boundary.
**Ratified mechanism:** Superi implements a native Rust OTIO JSON reader and writer, preserves unknown
schemas and fields as opaque versioned data, and validates compatibility against the official OTIO
reference implementation. The full contract lives in
[`phase-0-build-contracts.md`](phase-0-build-contracts.md#6-opentimelineio-interchange).

### 3.4 Application/UI layer: custom retained Rust and wgpu *(locked)*

**Decision:** Superi owns a native retained interface in Rust. One immutable scene supplies pixels,
hit testing, focus, semantics, and inspection output. winit hosts native windows, AccessKit bridges
assistive technology, and wgpu presents through the engine's existing GPU ownership. The UI drives
authored behavior through the **open automation API** (§3.5).

**Rationale:** A professional editor must combine dense interaction with real-time GPU media while
remaining deterministic, inspectable, and native-crisp. Sharing Rust types, wgpu resources, and the
single GPU submission owner avoids a privileged presentation transport and keeps bulk media out of
serialized command paths. Retained stable identities make visual output, input replay, accessibility,
and automated evidence different views of the same scene instead of separate implementations.

**Accepted costs:** Superi must build and maintain layout, text, accessibility, input, docking,
virtualization, and widget behavior deliberately. Those responsibilities are decomposed across Phase
Infinity and may reuse focused infrastructure libraries, but no general-purpose interface toolkit
owns product semantics or rendering.

### 3.5 The open automation API: the load-bearing seam *(locked: lives in the open core)*
**Decision:** The editor exposes a **public automation/control API that lives in the open MIT core**. It is the single surface through which the UI, user scripting, open third-party extensions, **and the proprietary agent** all drive the editor.
**Rationale & consequences:** This is one of the most consequential decisions in the system. It keeps the open editor genuinely complete and automatable on its own; it makes the boundary hard and clean (the closed agent is a *client* of an open surface, not a privileged backdoor); and it sets the correct one-way direction of dependence (agent depends on API, never the reverse). The proprietary agent's value was never "the ability to call editor functions", that's open plumbing anyone can use, it's the *intelligence deciding what to do*. Giving away the plumbing strengthens the open ecosystem (others can build open extensions/agents on it) while the proprietary moat (hosted intelligence + metered generation) is untouched.

---

## 4. Architectural commitments (the shape of the system)

These are the structural patterns every subsystem inherits.

### 4.1 Engine / application separation
A hard separation between a **headless, scriptable, testable engine** (decode, render, composite, color, audio, export) and the **application layer** (UI, project management, editing-operation logic, AI orchestration). The engine must be runnable without a GUI, renderable from a CLI in CI, frame-for-frame identical to the UI. This is what makes the engine testable, deterministic, and reusable, which is non-negotiable for a color-critical tool.

### 4.2 The node graph as the fundamental primitive
**Everything renders through a directed acyclic graph of nodes**, each node a GPU operation (decode, transform, color op, blend, effect, output). The **timeline is a high-level editing view that compiles down to graph operations**, a clip with effects is a subgraph; a color grade is a node; compositing is a node graph. This single decision is the architectural leverage that lets Superi be editor *and* compositor *and* color tool on **one engine** rather than three: later disciplines become *new node types on an engine that already evaluates them*, not new subsystems. Evaluation is **lazy, per-frame, per-region**.

### 4.3 GPU-resident, linear, high-bit-depth pipeline
Decoded frames go to GPU memory and **stay there** through the graph; readback to CPU only for export/thumbnails. Internally everything works in **linear, 16-bit-float color**, managed by the color substrate, with transforms in (footage-native → linear working) and out (working → display/delivery). Neither GPU-residency nor correct linear color can be retrofitted, they are foundational substrate.

### 4.4 Caching & proxies as first-class
Real-time playback is fundamentally a **caching achievement**. A frame cache (final frames + intermediate node outputs) and a proxy/optimized-media system are first-class engine concepts, not optimizations added later.

### 4.5 Threading model
Render and playback paths are **separated from the UI**, with a job system parallelizing frame/tile work and careful GPU command submission. Stutter-free playback is a concurrency achievement; Rust's Send/Sync guarantees do real work protecting the shared state (cache, buffers, clock).

### 4.6 The encumbered-codec boundary
A **media-I/O abstraction** isolates all codec-specific, potentially patent-encumbered code (FFmpeg/libav, patented codecs) behind a swappable interface. The MIT-clean core never directly links GPL or patented components; the codec layer is a separate, clearly-licensed, user-suppliable module. *(Requires legal confirmation before the decode layer is built, see Open Items.)*

---

## 5. Subsystem inventory: the bones

Complete map of the underlying technology the open editor must contain. **[FND]** = foundation-critical (cannot be retrofitted; shapes everything above). **[ADD]** = additive (layered onto the proven core; listed so it isn't forgotten, not built early). Built **pulled by the vertical slice in dependency order, not all at once in isolation** (see §6/§7).

### 5.1 Media I/O & codecs
[FND] decode interface (codec-agnostic "give me frame N"); [FND] encode interface; [FND] concrete decode/encode module behind the interface (the swappable, encumbered-isolated layer); demux/container parsing; timestamp/timecode + frame-accurate seeking; variable-frame-rate & drop-frame handling; audio stream demux/decode/encode; image-sequence I/O; [ADD] RAW/camera-format handling.

### 5.2 GPU pipeline (wgpu)
[FND] buffer & texture management; [FND] decoded-frame upload keeping frames GPU-resident; [FND] pixel-format conversions (YUV→working textures and back); [FND] shader infrastructure / compute & render pass orchestration; GPU memory pooling & pressure management; [ADD] multi-GPU/device selection; readback path (export/thumbnails only).

### 5.3 Color substrate (OCIO replacement: heavy rewrite)
[FND] linear 16-bit-float working space; [FND] transforms in; [FND] transforms out; [FND] configurable color-management config system (OCIO-config-equivalent); display/view transforms; HDR transfer functions & wide gamut; LUT loading/application (1D/3D); ICC/display-profile awareness.

### 5.4 Image primitives (OIIO/OpenEXR replacement)
[FND] HDR/high-bit-depth image data model (EXR-equivalent); [FND] core pixel ops (resize, crop, transform, blend, composite); wide format read/write breadth; tiled/scanline/mipmap handling; channel/layer model; image metadata model.

### 5.5 Node-graph engine (the core)
[FND] DAG data structure (nodes=GPU ops, edges=pixel flow); [FND] lazy per-frame/per-region evaluator; [FND] node I/O contract & type system; [FND] graph mutation API (the surface timeline & UI compile to); graph serialization/deserialization; ROI/dirty-region propagation; deterministic headless evaluation (CLI/CI render parity); expression/parameter-linking system (drives keyframing/rigging).

### 5.6 Caching & media optimization
[FND] frame cache (final + intermediate); [FND] proxy/optimized-media generation & substitution; render/background-render cache; prefetch & predictive caching; eviction & memory-budget policy; on-disk persistent cache.

### 5.7 Concurrency & scheduling
[FND] render/playback separated from UI thread; [FND] job system for parallel frame/tile work; [FND] GPU command submission & synchronization model; playback clock & A/V-sync scheduler; shared-state model leveraging Rust Send/Sync.

### 5.8 Timeline / editorial data model
[FND] Rust-native timeline model (tracks/clips/transitions/edit decisions); [FND] OTIO-compatible serialization (*do not strand on a proprietary format*); edit-op primitives (ripple/roll/slip/slide/razor/3-4-point); multicam model; markers/metadata/bins/media-management; nested sequences/compound clips.

### 5.9 Audio engine (its own subsystem, not a feature)
[FND] separate audio processing graph; [FND] sample-accurate A/V sync; [FND] low-latency playback path; [FND] mixing architecture (buses/levels/fades); sample-rate conversion/resampling; metering & analysis; [ADD] VST3/AU plugin hosting; [ADD] advanced mixing/routing/automation.

### 5.10 Effects & extensibility
[FND] internal effect/node authoring conventions; [FND] keyframing & animation system; masking/roto data model & rendering; transitions framework; [ADD] OFX-compatible plugin interface (inherit existing effects ecosystem); [ADD] text & motion-design primitives; [ADD] tracking/motion-tracking data + solver.

### 5.11 Scattered AI (open tier: local, offline, bundled, MIT)
> Open-tier AI only. Every item runs on a bundled, permissively-licensed model **entirely offline** (must survive the unplugged-cable test) and **transforms content the user already has, never generates new content**. The proprietary Superi Max tier is **not** here: its generation models are third-party services that exist out in the world (not bones we build), and its own buildable bones live in the **separate proprietary codebase across the boundary** (§8).

[FND] local inference runtime (bundled permissive models, on-device, **offline only**, no remote path); [FND] AI outputs as standard editable graph artifacts (mask nodes, color ops, edit decisions), never a black-box bypass of the graph; [FND] bundled-model licensing audit hook.

Per-feature pipelines (each its own buildable bone):
- Auto-captioning / transcription
- Audio denoising
- Silence detection & removal
- Filler-word detection
- Speaker diarization
- Background removal / subject masking
- Auto-reframe (re-crop H→V/square, keeping subject framed)
- Scene / cut detection
- Object & face tracking
- Auto color matching
- Content-based media search & tagging
- Transcript-based editing (*closest to the agent seam; kept scattered as a single deterministic transcript→cut mapping*)

### 5.12 Cross-cutting engine concerns
Project/document model & persistence; undo/redo model (engine-level command history); render/export pipeline & queue; logging/diagnostics/profiling; plugin/extension loading & sandboxing; scripting/automation API surface (**also the engine's public API, load-bearing, see §3.5**); settings/configuration system; crash recovery/autosave; color/format/codec capability introspection.

### 5.13 Orchestration & integration layer (engine code, NOT UI)
> The connective tissue binding the subsystems into one coherent system. **Not UI work**, some of the hardest, most bug-prone engine code, existing only by wiring the other bones together. A pile of individually-finished components is not a system until this exists. This is **Phase 2** in the build sequence (§6), and the *culmination* of the continuous integration that runs through Phase 1, not its start.

[FND] playback orchestration (decoder + graph + cache + audio + clock → synchronized real-time output); [FND] render/export orchestration flow (the actual end-to-end decode→graph→color→encode binding); [FND] A/V sync coordination under real conditions; [FND] engine-wide command/transaction model (undo, persistence, API all route through it); [FND] **the unified public engine API** (the single surface UI/scripting/extensions/agent drive, §3.5); [FND] subsystem lifecycle & shared-state management; [FND] cross-subsystem error propagation & recovery; [FND] cross-subsystem memory/resource arbitration (cache vs GPU buffers vs decode buffers for a finite budget); [FND] integration validation in concert under real conditions (format mismatches, sync races, color round-trips, memory interactions), engine-level work that happens *after* each component is individually "done."

The engine plugin supervisor recursively discovers validated OpenFX packages, delegates native
binary selection and operating-system containment to a platform launcher, and accepts only the
bounded isolated adapter contract. It narrows activation to each plugin's exact requested
capabilities, contains scan and worker failures per plugin, and projects one active graph registry
into playback, rendering, and export so authored nodes remain editable while unavailable plugins
fail closed.

---

## 6. Build phases (build-sequence framing)

> **The one principle that shapes everything: integration is continuous, not a later phase.** Do **not** build all parts in isolation and then integrate in a separate phase, that creates an "integration cliff" where untested assumptions surface all at once as massive rework. Instead, a **thin vertical slice** (import → single-track timeline → trim → one effect → export, GPU-accelerated) runs through the components *as they are built*, validating each part's shape by use the moment it exists. The slice is a **continuous-integration harness**, not a feature rush.

- **Phase 0, Planning & Decisions (now):** lock irreversible decisions ✅; produce architecture (this doc) + licensing/IP strategy; define the slice. *Exit: decisions locked + written architecture enabling credible hiring.* Team: founder + small senior founding team (5-8 specialists, heavy on graphics/codec/audio). Not more.
- **Phase 1, Build the Engine Parts (heavy engine code, WITH continuous integration):** build the §5 substrate (Rust-native, replacing the lost C++/ASWF ecosystem) in dependency order, pulled by the slice. *Exit: every required subsystem exists and the slice runs end-to-end, real-time GPU playback of 4K through the actual graph engine.*
- **Phase 2, Orchestration & Integration (engine code, the culmination of integration):** harden the continuous integration into a coherent, performant engine (§5.13). *Exit: a running, integrated, headless engine driven entirely through its public API, proven by the slice/CLI exercising that API throughout.*
- **Phase 3, The UI / The Actual Editor:** build the web-tech editor on the running engine, against the public API (never writing the underlying edit logic). The **capability progression** plays out here, each discipline additive because the node graph was built first:
  - *3a, v1, the Professional Timeline Editor* (first public-quality milestone): full editing ops, proxy workflow, foundational color (primary + scopes), real multi-track audio, reliable export, the scattered-AI set. *Bar: a working editor would choose Superi for a real cut and find it solid.*
  - *3b, Compositing & Motion* (After Effects axis): graph compositing, keyframed effects, masking/roto, text & motion design, OFX interface.
  - *3c, Advanced Color* (Resolve axis): node-based secondary grading, full color page, HDR, advanced scopes.
  - *3d, Pro Audio Maturity & Deeper AI*: VST/AU hosting & real mixing; expanded AI.
  - *3e, Unification*: the disciplines feel like one environment, not three modes.
- **Phase 4, Private/Beta Testing, Optimization & Finalization:** harden against real users, footage, edge cases; squeeze performance. (Integration testing is *not* first here, it's been continuous since Phase 1. Foundational performance, stutter-free playback, GPU-residency, was architected in from the start.) *Exit: stable, performant, ready for public use.*
- **Phase 5, Public Open-Source Launch:** the MIT, offline-complete public release; start of the asymptotic, never-"finished" life of the project.

**Two lenses:** the numbered phases above are the canonical *build-sequence* (how work/teams are sequenced, the shared vocabulary); the *capability progression* (engine → v1 → compositing → color → audio/AI → unification) is nested in Phase 3 (what Superi can do over time). "Phase 2" always means orchestration.

---

## 7. Engine-team-before-UI-team sequencing

The clean split is real and achievable: the engine team builds all §5 substance **and** the §5.13 orchestration first (Phases 1-2), producing a running, integrated, headless engine driven through its public API. Only then does the UI team build on top (Phase 3). The UI team genuinely never writes underlying edit logic.

**Caveat & refinement:** a *perfectly* serial hand-off ("engine 100% done, then hire UI, then start") is cleaner on paper than in practice, a public API designed with **zero real consumer** tends to be subtly the wrong shape, surfacing as rework when the UI arrives. The fix that keeps the clean split: let the **headless vertical slice + a thin scripting/CLI harness** act as the engine's "first consumer" throughout Phases 1-2, so the API is validated by real use before any UI exists. The engine→UI hand-off is then to a *proven, exercised* API. You still avoid running two large teams concurrently; the engine is simply *consumed* as it's built.

---

## 8. The proprietary tier (Superi Max): separate codebase across the boundary

> Defined here for completeness. Its **generation models are third-party services** (not bones we build, so not in §5). Its own **buildable bones** live in a separate proprietary codebase that attaches across the open API seam and **never lives in the open tree**. Everything it produces lands in the open editor as **ordinary, editable state** (a normal clip, edit, or node), so results survive the unplugged cable; only the *act* of generating/reasoning needs the network.

**Two products, one credit pool, one account system:**

1. **Media generation (manual), the headline; creates what never existed.** Invoked directly in the editor, third-party-model-powered, credit-metered, in four categories: **audio generation**, **image generation**, **video generation**, and **edit-media generation** (quick effects, transitions, animations, motion graphics, templates, generatable *editing vocabulary*, droppable anywhere like any other media; the most novel/least-matched capability; its boundary with image generation is intentionally soft). *Generation produces content/media only, never editorial operations.*

2. **The agent, a second version of you; accelerates the possible.** A general intelligence that edits the timeline exactly as a human would but **programmatically, driving the same open automation API** a human's UI actions drive; results land as ordinary editable state. Its proprietary value is the *reasoning*, not the ability to call editor functions. The agent: **uses every media-generation tool** the human can; **reaches outward with granular permission** (pulling existing assets from the user's computer and the internet, permission model deliberate about scope and about the rights of sourced material); and **does the editorial/post work** (trimming/cutting, audio mixing, organization, b-roll, color correction, VFX via the media tools, sound design).

**How they relate:** generation is the more **transformative** value (makes the impossible possible) and the stronger differentiator (agents are commoditizing; native generation of the whole editing vocabulary is not). The agent is the more **certain** value (reliable to build, high floor even when imperfect) and **generation's most important consumer** (what draws from the well at scale while doing everything else). Complementary, not ranked, both matter, together.

**Proprietary buildable bones (separate codebase):** agent orchestration/reasoning integration; account system; credit pool & metering (must handle both predictable per-generation costs *and* the agent's variable multi-step cost, which can itself trigger generations); the outward-reach permission model; the editor-control integration (as a client of the open API); server infrastructure. *(These are noted, not specified here; this document's primary scope is the open editor.)*

---

## 9. Brand & visual direction

Superi's foundation direction is **Obsidian Signal**: pure black working surfaces, precise seams,
compact Inter typography, sparse semantic color, original geometric icons, and information-dense
native panels. Cyan indicates active control and navigation, while violet, green, amber, and red are
reserved for stable semantic roles.

The color-critical environment maintains low surround luminance and keeps interface chrome separate
from media color. Later design checkpoints expand the system through evidence-backed primitives
rather than copying another product's artwork or control geometry.

---

## 10. Open items (honestly unresolved: owned by the founding team)

These are **deliberately open**, and the document is stronger for marking them so rather than papering over them. None blocks starting; each must be closed before the work it gates.

1. **Codec-boundary legal review** *(blocks the decode layer).* Confirm with an IP lawyer that the media-I/O boundary holds, that the MIT core never links GPL/patent-encumbered code, and map the patent landscape (H.264/H.265/ProRes/AAC) so it's clear what lives inside vs. outside the tree. The full-MIT promise depends on this.
2. **Agent metering model** *(proprietary tier, later).* How variable-cost agentic work (multi-step reasoning that can itself trigger generations) is metered comprehensibly and predictably for users.
3. **Permission model for the agent's outward reach** *(proprietary tier, later).* Granular (per-folder/source/action) consent, plus deliberate handling of the rights/licensing of internet-sourced material pulled into commercial projects.

---

## 11. The decisions in one glance

| Area | Decision | Status |
|---|---|---|
| Engine language | Rust | Locked |
| Graphics | wgpu (Vulkan/Metal/D3D12) | Locked |
| Timeline model | Rust-native, OTIO-compatible | Locked; native Rust OTIO JSON interchange |
| UI / application | Custom retained Rust scene, winit, wgpu, and AccessKit | Locked |
| UI to engine seam | Open automation API in the MIT core | Locked |
| Licensing (core) | Full MIT; encumbered codecs isolated behind media-I/O boundary | Locked (legal review pending) |
| Core render primitive | Node graph; timeline compiles to it | Locked |
| Color | Linear 16-bit float; OCIO-equivalent rebuilt in Rust | Locked |
| Open/closed AI line | Transform-what-exists (open/local) vs generate-the-nonexistent (closed/server) | Locked |
| Hard boundary | Open editor fully works offline; enforced by network-isolated CI + license audit | Locked |
| Custom GPU UI | Retained native scene over the sole wgpu owner | Active foundation |
| Brand | Obsidian Signal with retained Superi marks | Direction locked, system expanding |
| Typography | Inter 4.1 seed with explicit fallback work | Locked seed |

---

*End of v0.2. This document consolidates the foundational decisions and is expected to evolve as the vertical slice is built and the founding engineers pressure-test its assumptions. The governing test remains: does this serve the North Star, nothing more, nothing less?*
