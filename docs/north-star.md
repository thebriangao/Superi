# Superi: North Star (The Final Build)

The destination Superi is built toward. Every downstream decision, architecture, dependencies, phases, code, must serve what is defined here, and the test it imposes on everything built afterward is: *does this serve the north star, nothing more, nothing less?*

---

## What Superi is

Superi is a professional post-production environment delivered in **two tiers** divided by one hard, physical boundary:

- **Superi (open, MIT)**, a complete, professional editor that runs entirely on the user's own machine, plus a set of local AI conveniences. Free, open-source, forkable, and **fully functional with the network physically unplugged**, no account, no servers, no credits, no degradation.
- **Superi Max (closed, proprietary)**, a server-backed, account-gated, credit-metered layer that attaches to the open editor: manual media generation and an orchestrating agent. This is the commercial product and the business.

The relationship is VSCode-to-Cursor: the open editor is genuinely complete and powerful on its own; the proprietary layer adds server-dependent intelligence on top. The open tier drives adoption and community; the closed tier drives revenue. **Most people will be glad to use Superi open; those who want Superi Max will be delighted too.**

> **Naming discipline (positioning, not just labels):** the open product is proudly **Superi**, never "Superi Free," "Superi Community," or any qualifier that implies it is the lesser/incomplete version. The paid layer is **Superi Max**. All messaging frames Superi as a *complete* professional editor and Superi Max as an *additive* layer ("Superi is a full editor; Superi Max adds generation and the agent"), never as a tier ladder where the free version is the crippled starter. The entire open-source strategy depends on Superi-unqualified being perceived as genuinely complete; the "Max" suffix must read as *companion*, not as *the real one*.

## The hard boundary (non-negotiable)

**Unplug the network, and the open editor must work end to end**, all editing, all local AI, full project save/load, no account, no degradation. If it works, the boundary holds; if anything reaches for a connection, that is a breach. Enforced as a network-isolated CI test so it can never silently erode, paired with a license audit proving every bundled component is genuinely permissively licensed. The open tree **never** imports, links, or depends on the closed tree, and must build and run completely without it.

The conceptual line that governs what goes where, and the core value insight of the whole project: **transform what already exists → open and local; generate what never existed → closed and server-backed.** This line is drawn on principle, not on what is momentarily possible locally, so it stays legible even as local models improve.

---

## SUPERI (open, MIT): the editor that makes it Superi

A post-production environment that, on editing capability alone (AI aside), matches DaVinci Resolve, Premiere Pro, and After Effects, four disciplines on one shared node-graph engine:

- **Editing**, real multi-track timeline; full editing operations (ripple, roll, slip, slide, razor, 3/4-point, snapping, markers); multicam; nested sequences/compound clips; professional media management with proxies and relinking.
- **Compositing & motion**, layered/graph-based compositing; keyframed animation with easing and expressions; masking and rotoscoping; text and motion graphics; motion tracking; effects (built-in and third-party plugins).
- **Color**, node-based primary and secondary grading; scopes; HDR and wide-gamut; correct color management throughout (linear high-bit-depth working space, accurate transforms in and out, LUTs).
- **Audio**, multi-track editing and mixing; levels, fades, routing; sample-accurate sync; metering; third-party audio plugins.

Plus the qualities that make it real: **real-time GPU playback** of HD/4K/8K, **cross-platform** (Mac/Windows/Linux) on native GPU, **stable over hours-long sessions**, **scriptable** through a public automation API, and **identity-free** (no login, no profile, it doesn't know the servers exist).

### Scattered AI (open, local, free: transforms what the user already has)

Local, permissively-licensed, bundled models doing single bounded tasks, conveniences that make the open editor genuinely good without ever generating new content (so they never compete with the paid tier):

- **Auto-captioning / transcription**, captions and subtitles from existing dialogue
- **Audio denoising**, remove hiss, hum, background noise
- **Silence detection & removal**, trim dead air and long pauses
- **Filler-word detection**, flag/remove "um," "uh," "like"
- **Speaker diarization**, auto-label who is speaking when
- **Background removal / subject masking**, isolate a subject from its background
- **Auto-reframe**, re-crop horizontal → vertical/square, keeping the subject framed
- **Scene / cut detection**, split a long clip at shot changes
- **Object & face tracking**, track a subject to drive masks and effects
- **Auto color matching**, match two existing shots for consistency
- **Content-based media search & tagging**, search your own footage by what's in it, no manual labeling
- **Transcript-based editing**, edit the video by editing its transcript

*Every one operates only on content the user already has. A feature qualifies as scattered only if it runs on a bundleable, permissively-licensed model offline; otherwise it belongs in the closed tier.*

---

## SUPERI MAX (closed, proprietary): what makes it Superi Max

The server-backed, credit-metered layer. Two distinct products sharing one credit pool and one account system. Everything it produces lands in the open editor as **ordinary, editable state**, a normal clip, a normal edit, a normal node, so what it leaves behind survives the unplugged cable like any other editor content; only the *act* of generating or reasoning needs the network.

### 1. Media generation (manual): the headline; creates what never existed

The deepest source of novel value in Superi: content that has no manual alternative at any amount of time. Invoked directly in the editor by the user, powered by third-party models, credit-metered, in four categories:

- **Audio generation**, music, sound, audio effects
- **Image generation**
- **Video generation**
- **Edit-media generation**, quick effects, transitions, animations, motion graphics, templates: generatable *editing vocabulary*, droppable anywhere like any other media (the most novel, least-matched capability; its boundary with image generation is intentionally soft)

*Generation produces content/media only, never editorial operations. Cutting, trimming, and arranging are the human's or the agent's job, never a generation tool's.*

### 2. The agent: a second version of you; accelerates the possible

A general intelligence that edits the timeline exactly as the human would, but programmatically (driving the same open automation API a human's UI actions drive), with results landing as ordinary editable state that undoes and refines like a human's. Its proprietary value is the *reasoning*, what to do and when, not the ability to call editor functions (that surface is open). The agent:

- **Uses every media-generation tool** the human can, generating a clip, an effect, an audio bed, a graphic when the needed content doesn't exist
- **Reaches outward, with granular permission**, pulling existing assets from the user's computer and the internet (a permission model deliberate about scope and about the rights of sourced material)
- **Does the editorial and post work**, trimming/cutting, audio mixing, organization, b-roll sourcing and placement, color correction, visual effects (via the media tools), sound design (via generation or direct editing), and more

### How the two relate

Media generation is the more **transformative** value (it makes the impossible possible) and the stronger differentiator (agents are commoditizing; native generation of the whole editing vocabulary is not). The agent is the more **certain** value (reliable to build, high floor even when imperfect) and, crucially, generation's most important *consumer*, the thing that lets that well be drawn on at scale. They are complementary, not ranked: generation is the deeper well; the agent is what draws from it while doing everything else. Both matter, and they matter together.

---

## The honest framing of "done"

Matching three flagship tools that each carry decades of development is **asymptotic**, Superi approaches the bar continuously rather than ticking it off. So the north star is defined not as feature-for-feature parity with any one of them, but as **the threshold at which a working professional can genuinely live in Superi for the majority of real projects** and choose it on its merits, with its openness, and the create-the-nonexistent power of Superi Max, as decisive advantages.

**Non-goals** (as defining as the goals): not chasing every niche format, broadcast/finishing workflow, or esoteric plugin on day one; not a thin wrapper over another engine (the engine is genuinely Superi's own); and never sacrificing the MIT-clean offline-complete core, the transform-vs-generate boundary, or the editable-artifact principle for any feature, however attractive.

**The thing that has never existed:** a flagship-quality post-production environment that is genuinely free and open and works entirely offline, with a proprietary generation-and-agent layer on top for those who want it. That is the destination, and the reason it is worth the funding and the years.
