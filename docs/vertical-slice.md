# Superi canonical vertical slice

**Status:** Normative contract, revision 1  
**Scenario ID:** `superi.slice.canonical.v1`  
**Owner:** Repository infrastructure and the continuous vertical slice

## 1. Purpose

This document defines the first editorial thread that every Superi subsystem must keep working:
import one clip, place it on one video track, trim it, apply one graph effect, and export it. It is
the single contract shared by the headless runner, fixtures, subsystem integration, and future UI.

The contract is intentionally exact without claiming that the runtime exists today. Media
contracts, a Matroska and WebM demuxer, AV1 decode and encode, color transforms, and CPU-frame GPU
upload exist in isolation. Production source registration, project state, timeline compilation,
graph evaluation, effect nodes, export orchestration, muxing, the general public command surface,
and the scenario runner do not yet form a runtime slice.

Revision 1 has two conformance levels:

- `contract` means the scenario and every stage boundary execute, with every stub disclosed.
- `runtime` means every stage uses its production owner and no stub appears in the report.

A contract-conformant run is useful continuous integration evidence. It must never be described as
a working editor, a real import-to-export run, or runtime conformance.

## 2. Canonical source

The source is immutable fixture `slice/video-cfr`, version 1, stored under the repository fixture
root. Its payload is `input.webm`. The fixture manifest, not a bare payload path, is the authority.
The runner must validate the manifest and exact payload digest before opening the clip.

The fixture contract is:

| Property | Required value |
| --- | --- |
| Origin | Deterministically generated, redistributable synthetic media |
| Container backend | `mkv-webm` |
| Video codec backend | `rust-av1` |
| Raster | 96 by 54 pixels |
| Rate and timebase | 24 frames per second, timebase 1/24 |
| Duration | 96 contiguous frames, source range `[0, 96)` |
| Picture | Opaque 8-bit 4:2:0 limited-range video with BT.709 primaries, transfer, and matrix |
| Pattern | Spatially asymmetric and temporally changing so trim and mirror errors are visible |
| Audio | None |

No extension-based dispatch, network fetch, implicit latest version, platform codec, fallback
backend, or fixture regeneration is permitted during a run. The fixture checkpoint owns the
payload and generator details. This contract owns its identity and semantics.

## 3. Editorial state

The scenario uses exact rational time and half-open ranges. One timeline named `canonical` has a
24 fps edit rate, a 96 by 54 canvas, exactly one video track named `V1`, and no audio tracks. One
clip instance named `clip-1` references the fixture media identity.

The untrimmed clip maps source `[0, 96)` to timeline `[0, 96)`. The trim operation sets source range
`[24, 72)` and timeline range `[0, 48)`. The final sequence therefore begins at timeline tick 0,
ends before tick 48, lasts exactly two seconds, and produces exactly 48 video frames.

The state mutation log is ordered and stable:

1. `slice.op.import` creates the persistent media identity from the validated fixture.
2. `slice.op.insert` creates `V1`, inserts `clip-1`, and records the untrimmed mapping.
3. `slice.op.trim` changes only the clip source and timeline ranges to the final values.
4. `slice.op.effect` attaches the one effect instance described below.

Every mutation must use the engine transaction path when that owner exists. Each record must retain
typed arguments, the resulting state revision, and enough prior state for the action to be reversed
without reimporting media or reconstructing intent from rendered pixels. Export is not a project
mutation and is not part of the undo log.

## 4. Graph state and the one effect

The final graph has exactly these stable node instances and edges:

```text
slice.node.source -> slice.node.effect -> slice.node.output
```

`slice.node.source` resolves `clip-1`, decodes the requested source tick, applies the explicit input
color transform, and exposes canonical premultiplied ACEScg RGBA binary16 working storage.
`slice.node.output` applies the independent deliverable color rule and owns explicit conversion to
the encoder input representation. These are source and output orchestration nodes, not additional
effect nodes.

`slice.node.effect` is the only effect-catalog node:

| Field | Required value |
| --- | --- |
| Node type | `superi.effect.transform` |
| Node schema | 1 |
| Input port | `image`, typed canonical working image |
| Output port | `image`, typed canonical working image |
| Parameter | `matrix`, typed finite row-major 3 by 3 binary64 matrix |
| Matrix | `[-1, 0, 95, 0, 1, 0, 0, 0, 1]` |
| Sampling | Nearest |
| Edge mode | Transparent black |
| Output extent | Fixed 96 by 54 canvas |

Pixel centers use integer coordinates. The matrix maps `(x, y)` to `(95 - x, y)`, so the effect is
an exact horizontal mirror with no resampling ambiguity inside the canvas. The source is opaque,
and every mapped sample remains inside the canvas.

Graph state must satisfy all of these rules:

- Node types, schema versions, ports, parameters, connections, and stable instance IDs are typed.
- The effect parameter remains ordinary editable project state, not a baked frame or CLI-only flag.
- Stable serialization order and exact numeric values make repeated state snapshots deterministic.
- Inspection returns the complete nodes, edges, parameter values, derived timeline identity, and
  state revision through the public control surface.
- The timeline compiler derives routing with stable IDs while retaining the authoring effect
  instance. Editing that instance changes the same graph consumed by preview and export.
- UI state, display state, cache contents, worker order, and backend discovery are not graph inputs.
- Bulk frames and GPU objects stay behind the engine and never cross the public API.

There is no second visual model. A future visual graph editor, timeline inspector, CLI, script, and
export worker must observe and edit this same state.

## 5. Deliverable

The exported artifact is `canonical.webm`. It contains one video stream and no audio stream. It has
96 by 54 display and coded dimensions, 24 fps timing, timeline range `[0, 48)`, and exactly 48
frames.

The deliverable rule is named `slice-sdr`. It accepts the scene-referred ACEScg working image,
applies no creative look, and performs the explicit deliverable output transform to sRGB. Storage
conversion then produces opaque 8-bit 4:2:0 YUV with a BT.709 matrix and limited range while
retaining the sRGB transfer and BT.709 primaries. Encoding uses AV1 through backend `rust-av1`, and
the deterministic WebM muxer writes stable track ordering and timestamps.

The selected source, decoder, encoder, and muxer are part of the report. A missing capability is an
explicit unsupported stage. The runner must not substitute an operating-system codec, another
backend, a different pixel contract, or an unreported fallback.

Exact expected hashes, pixel and audio tolerances, timestamp expectations, project metadata, and
export metadata belong to the versioned expectation record defined by P1.W07.C024. Until that
record exists, a run can prove exact state, timing, stage identity, frame count, and structural
metadata, but it cannot claim golden-output conformance.

## 6. Stable stages

The scenario always reports these stages in this order:

| Stage ID | Production owner | Required output |
| --- | --- | --- |
| `fixture.resolve` | Workspace fixture contract | Validated immutable fixture reference and payload digest |
| `media.import` | `superi-media-io`, codecs, and engine assembly | Persistent media identity plus exact stream and timing description |
| `timeline.edit` | Project and timeline state | One-track sequence, trimmed clip, ordered reversible operations |
| `timeline.compile` | Timeline compiler | Stable derived graph routing and source-time mapping |
| `graph.evaluate` | Graph, effects, color, GPU, and cache owners | 48 effect-processed canonical working frames |
| `color.deliver` | Color rules and storage conversion | Explicit delivery-tagged encoder frames |
| `media.export` | Engine export, AV1 encoder, and WebM muxer | `canonical.webm` with exact stream structure and timing |
| `slice.verify` | Scenario verifier | State, stage, fixture, export, and expectation evidence |

Every stage record includes `stage_id`, `implementation` (`stub` or `runtime`), owning component,
implementation revision, typed input summary, typed output summary, duration, and diagnostics. A
stub may model only its own typed boundary. It may not claim a production backend ran, hide an
existing real failure, bypass fixture validation, or mark the run as runtime conformant.

Subsystem work replaces the corresponding stub without changing the scenario ID, editorial
meaning, state ownership, stage order, or report contract. A scenario revision is required for an
intentional contract change. The runner must reject an unknown revision instead of guessing.

## 7. Headless runner contract

P1.W07.C017 owns the implementation of this normalized invocation:

```text
superi-cli slice run --scenario superi.slice.canonical.v1 \
  --artifact-dir <empty-directory> --report <report.json>
```

The default build and default feature set are mandatory. The process runs with the network
unavailable and resolves all input from the repository. It writes no source fixture, project input,
or expected-output record. The artifact directory must be empty or absent at start, and publication
of the export and report must be collision-safe.

`report.json` is a strict versioned record with at least these fields:

- report schema version, scenario ID, and scenario revision;
- repository commit and dirty-state flag;
- fixture ID, fixture version, manifest digest, and payload digests;
- deterministic project, timeline, graph, and operation-log state digests;
- all eight ordered stage records and their implementation kinds;
- selected backend identities, feature set, target, toolchain, and build profile;
- export path, byte count, digest, stream description, frame count, and timestamp summary;
- expectation-record identity and every exact or tolerance-based comparison result;
- overall conformance level and stable structured diagnostics.

Success exits 0 only when fixture validation, every stage, artifact publication, and all available
expectations pass. Contract-only success must say `contract`, list at least one stub, and never say
`runtime`. Invalid input exits 2. An unavailable or unsupported required capability exits 3. A
stage, export, or verification failure exits 4. Diagnostics must identify the failed stage and
retain the repository error category and recoverability when available.

## 8. Reproducibility and proof

A reproducible run requires the same repository commit, locked dependency graph, default feature
set, fixture manifest digest, scenario revision, and expectation-record revision. Contributors use
locked Cargo commands. CI records the complete identity tuple rather than relying on a branch name
or the word latest.

The minimum proof for every revision is:

1. Validate all fixture manifests and payload digests offline.
2. Build the workspace with the lockfile and default features.
3. Run the scenario twice into different empty artifact directories.
4. Compare state digests, stage order and identities, timing, frame count, diagnostics, and
   expectation results.
5. Inspect the serialized timeline and graph and confirm the one track, one clip, exact trim, one
   effect, typed matrix, three nodes, and two edges.
6. Replay the inverse effect and trim mutations, then replay the forward mutations and recover the
   exact final state digest without reimporting the fixture.
7. Decode the export through the selected default backend and compare it with the versioned
   expectations.

Cross-platform results follow `docs/platform-testing.md`: deterministic contract and state values
must match exactly, while media comparisons use only the explicit tolerances in the expectation
record. A skipped stage is a reported platform gap, not a pass. A retry keeps the original failure
evidence.

## 9. Adjacent checkpoint boundaries

Revision 1 is video-only and deliberately small. Nearby checkpoints extend its evidence without
silently changing this baseline:

- P1.W07.C017 implements the CLI scenario runner and strict report.
- P1.W07.C018 supplies the canonical deterministic video fixture.
- P1.W07.C019 adds synchronized multichannel audio fixtures and audio coverage.
- P1.W07.C020 adds CFR, VFR, drop-frame, and discontinuous-timestamp timing fixtures.
- P1.W07.C021 adds color, HDR, alpha, high-bit-depth, and image-sequence fixtures.
- P1.W07.C022 adds malformed, truncated, unsupported, and partial inputs.
- P1.W07.C023 adds OTIO fixtures.
- P1.W07.C024 supplies versioned expected outputs, hashes, tolerances, and metadata.
- P1.W07.C025 adds timing and memory instrumentation.
- P1.W07.C026 replaces each disclosed stub with its production subsystem.

Multitrack editing, transitions, playback, UI interaction, operating-system codecs, vendor codecs,
network services, hosted fallback, and performance targets are outside this scenario revision.
