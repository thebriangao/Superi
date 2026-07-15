---
module_id: superi-core
source_paths:
  - open/crates/superi-core
source_hash: 4e084e9c67bddf2afba781a5f525904868d46c3611d90924a91167ed2ae99991
source_files: 23
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-core` is the tier-zero shared contract crate for Superi. It owns platform-neutral value
types, validation rules, stable codes, exact time and geometry semantics, the common error and
diagnostic vocabulary, and the revisioned Serde representation used at project, engine,
extension, automation, and process boundaries. It deliberately has no dependency on another
Superi crate.

The crate owns meaning and interchange, not subsystem policy or runtime resources. Identifier
allocation remains with the subsystem that owns the identified object. Color conversion belongs
to `superi-color`; image storage and operations belong to `superi-image`; audio mixing and
resampling belong to audio owners; media probing, decoding, and encoding belong to media and codec
crates; GPU resources remain backend-owned. The core pixel, sample, color, geometry, and time types
describe those resources without taking ownership of their buffers or execution.

The library-level documentation still says `Status: skeleton`, but the implementation is not a
skeleton. It is a substantive public contract with 11 source modules, strict constructors and
parsers, a stable wire revision, 10 integration-test targets, and active consumers across the
workspace.

## Source inventory

The module owns 23 text files under one crate path:

- `open/crates/superi-core/Cargo.toml` declares the tier-zero library, its sole runtime dependency
  on workspace `serde`, and the `serde_json` test dependency.
- `open/crates/superi-core/src/lib.rs` is the public module root. It exposes `color_space`,
  `diagnostics`, `error`, `geometry`, `ids`, `pixel`, `prelude`, `serialization`, `settings`,
  `time`, and `timecode`.
- `open/crates/superi-core/src/color_space.rs` defines independent color primaries, transfer,
  matrix, and range tags plus composed `ColorSpace` constants. It preserves unusual declared
  combinations without normalizing or validating operational support.
- `open/crates/superi-core/src/diagnostics.rs` defines typed tracing values and visibility,
  deterministic diagnostic events, raw failure snapshots, user-safe error projections, and
  thread-safe saturating performance counters.
- `open/crates/superi-core/src/error.rs` defines the cross-subsystem error category and
  recoverability vocabularies, ordered context frames, source-preserving `Error`, the canonical
  `Result<T>`, and `ResultExt`.
- `open/crates/superi-core/src/geometry.rs` defines finite points, vectors, homogeneous 3 by 3
  transforms, half-open continuous rectangles, reduced positive aspect ratios, and signed
  half-open pixel bounds with checked arithmetic.
- `open/crates/superi-core/src/ids.rs` defines twenty-two opaque 128-bit typed identifiers, their
  sealed common trait, permanent domain prefixes, big-endian byte form, and strict lowercase
  hexadecimal parsing. Timeline, gap, transition, generator, and caption are the editorial
  additions, including editorial marker, media bin, smart collection, and multicam-angle domains;
  graph, port, edge, and resource are the graph-facing additions.
- `open/crates/superi-core/src/pixel.rs` defines pixel storage tags, alpha interpretation, audio
  sample storage tags, semantic channel positions, and immutable ordered channel layouts.
- `open/crates/superi-core/src/prelude.rs` is an intentionally reviewed allowlist of broadly shared
  contracts. It re-exports ordinary value and error types while excluding construction-specific
  parse errors, internal failure snapshots, and lower-level version parse details.
- `open/crates/superi-core/src/serialization.rs` owns stable Serde implementations for the public
  primitive values and exports `STABLE_PRIMITIVE_SCHEMA_REVISION`. It uses permanent string codes,
  explicit object fields, checked reconstruction, decimal strings for wide integers, and unknown
  field rejection.
- `open/crates/superi-core/src/settings.rs` defines canonical namespaced keys and identifiers,
  Semantic Versioning 2.0.0 values, component-qualified versions, typed settings snapshots,
  symbolic capability sets, and validated feature discovery snapshots.
- `open/crates/superi-core/src/time.rs` defines reduced positive timebases and frame rates, signed
  exact rational coordinates, explicit rounding, sample clocks, nonnegative durations, and
  half-open time ranges.
- `open/crates/superi-core/src/timecode.rs` defines strict signed editorial timecode over a
  continuous physical frame index, including non-drop and supported NTSC-related drop-frame
  label conventions.
- `open/crates/superi-core/tests/diagnostics_contract.rs` proves stable diagnostic codes, name and
  finite-number validation, deterministic field order, visibility filtering, safe error
  projection, source retention, counter saturation, concurrency, and thread-safety traits.
- `open/crates/superi-core/tests/error_contract.rs` proves category and recovery codes, context
  ownership and order, display order, source chains, result-context preservation, and `Send` plus
  `Sync` behavior.
- `open/crates/superi-core/tests/geometry_contract.rs` proves point and vector distinction,
  finiteness, checked arithmetic, half-open bounds, exact aspect ratios, matrix order, projective
  horizon handling, inversion, and transformed bounds.
- `open/crates/superi-core/tests/id_contract.rs` proves type separation, 16-byte layout,
  endianness, ordering, canonical text, strict parser rejection, and stable identifier domains.
- `open/crates/superi-core/tests/media_tag_contract.rs` proves pixel and sample metadata,
  planarity, subsampling, alpha separation, ordered channel identity, color-axis preservation,
  permanent code uniqueness, and thread-safety traits.
- `open/crates/superi-core/tests/prelude_contract.rs` proves the curated re-export set, Serde trait
  availability, error extension use, and a prelude-only cross-domain JSON round trip.
- `open/crates/superi-core/tests/serialization_contract.rs` proves stable trait coverage, exact
  wire codes and shapes, wide-integer decimal strings, deterministic ordering, checked composite
  reconstruction, malformed-input rejection, and nested public-consumer round trips.
- `open/crates/superi-core/tests/settings_contract.rs` proves canonical shared names, semantic
  version identity and precedence, setting snapshot ordering and duplicate rejection, symbolic
  capability semantics, feature consistency, stable codes, and thread-safety traits.
- `open/crates/superi-core/tests/time_contract.rs` proves reduced clocks, professional frame-rate
  constants, physical-time equality, every explicit rounding mode, checked arithmetic, sample and
  frame alignment, bounded duration, and half-open range behavior.
- `open/crates/superi-core/tests/timecode_contract.rs` proves canonical signed labels, drop-frame
  skipped-label behavior at 30000/1001 and 60000/1001, rate rejection, checked arithmetic,
  explicit conversion rounding, long positions, and representative label round trips including
  `i64` extrema.

## Public surface

The crate exposes named modules directly and provides a curated `prelude` for common contracts.
No external workspace source currently imports the prelude; active consumers use owning module
paths, so those paths are the effective public boundary today.

The major surfaces are:

- Errors: `ErrorCategory`, `Recoverability`, `ErrorContext`, `Error`, `Result<T>`, and `ResultExt`.
  `Error` can retain any standard `Send + Sync` source and appends local context from the failing
  operation outward. Its display reverses stored contexts to show the outermost caller first.
- Diagnostics: `DiagnosticSeverity`, `FieldVisibility`, `FiniteF64`, `TraceValue`, `TraceField`,
  `FailureDiagnostic`, `UserSafeError`, `DiagnosticEvent`, `CounterUnit`, `CounterSnapshot`, and
  `PerformanceCounter`. `FailureDiagnostic` is public on its module path but intentionally omitted
  from the prelude because it may contain sensitive raw data.
- Identifiers: `ProjectId`, `MediaId`, `BinId`, `SmartCollectionId`, `TrackId`, `ClipId`,
  `TimelineId`, `GapId`, `TransitionId`, `GeneratorId`, `CaptionId`, `MarkerId`, `NodeId`,
  `ParameterId`, `JobId`, `CacheId`, `DeviceId`, `GraphId`, `PortId`, `EdgeId`, `ResourceId`,
  `MulticamAngleId`, `IdentifierKind`, sealed `TypedId`, and
  `ParseIdentifierError`.
- Time: `Timebase`, `FrameRate`, `TimeRounding`, `RationalTime`, `SampleTime`, `Duration`, and
  `TimeRange`. Common exact frame-rate constants include integer rates and 24000/1001,
  30000/1001, and 60000/1001.
- Timecode: `TimecodeMode`, `TimecodeFormat`, `TimecodeComponent`, `TimecodeError`, and `Timecode`.
  Parse and conversion failures remain a specific error type rather than being flattened into the
  shared `Error`, except that underlying rational-time failures are retained in `TimecodeError::Time`.
- Geometry: `Point2`, `Vector2`, `Matrix3`, `Rect`, `AspectRatio`, and `PixelBounds`.
- Image and audio representation: `PixelModel`, `PixelNumeric`, `PixelPacking`,
  `ChromaSubsampling`, `PixelFormat`, `AlphaMode`, `SampleNumeric`, `SampleFormat`,
  `ChannelPosition`, and `ChannelLayout`.
- Color interpretation: `ColorPrimaries`, `TransferFunction`, `MatrixCoefficients`, `ColorRange`,
  and `ColorSpace`, including constants for unspecified, sRGB, BT.709, BT.2020, BT.2100 PQ and
  HLG, Display P3, ACES2065-1, and ACEScg interpretations.
- Settings and discovery: `SettingKey`, `CapabilityId`, `FeatureId`, `ComponentId`, their parse
  errors, `SemanticVersion`, `VersionIdentifier`, `SettingValueKind`, `SettingValue`,
  `SettingsSnapshot`, `CapabilitySet`, `FeatureAvailability`, `FeatureDescriptor`, and
  `FeatureDiscovery`.
- Serialization: `STABLE_PRIMITIVE_SCHEMA_REVISION`, currently `1`, plus Serde implementations on
  the stable values. The runtime `Error`, specific parser error enums, `TimecodeError`, and mutable
  `PerformanceCounter` are not wire values; callers serialize diagnostic projections and
  `CounterSnapshot` instead.

Most public tag enums are `non_exhaustive`, while their `ALL`, `code`, and `from_code` APIs expose
the complete set known to the current version. Concrete state is generally private and reachable
through checked constructors and read-only accessors. The typed identifier trait is sealed so an
external crate cannot claim a custom type is an official core identifier domain.

## Architecture and data flow

### Construction and validation

Public constructors fall into three groups. Pure tags and already-valid opaque values use `const`
constructors. Domain values such as timebases, geometry, channel layouts, settings snapshots, and
feature discovery route through validation and return the shared `Result`. Canonical text types
and timecode use specific parse errors so callers can identify the exact rejected section.

The shared-error path is:

1. A constructor or downstream subsystem creates `Error` with an explicit category,
   recoverability, and raw diagnostic summary.
2. The failing operation and outer callers append ordered `ErrorContext` frames. Context fields are
   stored in `BTreeMap` order and may contain sensitive details.
3. `DiagnosticEvent::from_error` snapshots the raw category, recovery, summary, contexts, and full
   standard source chain into `FailureDiagnostic`.
4. The same event independently derives `UserSafeError` only from category and recoverability.
   Raw summaries, context values, paths, and source strings are not copied into this projection.
5. Presentation code must consume `UserSafeError` and `user_safe_fields`; internal diagnostic
   pipelines may consume the full event.

### Exact time and timecode

`Timebase` stores a reduced positive rational number of units per second. A `RationalTime` value
represents `value * denominator / numerator` seconds. Equality and ordering compare physical time,
not the stored pair. Rescaling and binary arithmetic require a target timebase and an explicit
`TimeRounding`, so no frame or sample conversion silently rounds.

`SampleTime` restricts audio clocks to a nonzero integral sample rate. `Duration` is nonnegative
and capped at `i64::MAX` so all arithmetic shares the signed coordinate domain. `TimeRange` stores
one exact start and same-timebase duration and treats its end as exclusive. Queries may compare a
time expressed at another rate because `RationalTime` comparison is physical-time based, but range
construction never performs an implicit timebase conversion.

`Timecode` stores a signed continuous physical frame index plus `TimecodeFormat`. Formatting is a
projection; drop-frame mode skips labels, never frames. Parsing validates the explicit separator,
component widths, component ranges, forbidden drop labels, negative zero, and checked arithmetic.
Conversion first projects to `RationalTime`, then applies the caller's explicit rounding policy at
the target frame rate.

### Media, color, and geometry representation

Pixel storage, alpha association, and color interpretation are independent values. `PixelFormat`
reports model, numeric representation, packing, meaningful bit depth, plane count, packed bytes
per pixel when applicable, chroma subsampling, and alpha presence. It does not carry dimensions,
row strides, plane buffers, alpha mode, or color space. Audio uses the same separation:
`SampleFormat` describes numeric storage and planarity, while `ChannelLayout` preserves a validated
ordered semantic position sequence.

`ColorSpace` is a lossless metadata tuple. Core does not infer or execute transforms and does not
reject unusual combinations. Downstream color, codec, media, image, engine, and GPU owners decide
whether an operation supports a tuple.

Continuous geometry permits only finite `f64` inputs and checked finite results. Rectangles and
pixel bounds are half-open. Matrices are row-major, multiply column vectors, place translation in
the final column, and name application order through `checked_then`. Projective horizon points,
horizon-crossing rectangle transforms, noninvertible matrices, and integer overflow are rejected.

### Settings, discovery, and serialization

Shared names have at least two dot-separated lowercase ASCII segments. Segment tails may contain
lowercase letters, digits, underscore, or hyphen. Semantic versions implement the complete core,
pre-release, build-metadata, identity, and precedence distinctions. Settings snapshots and feature
discovery own immutable, versioned, deterministic collections. Available features must have every
declared required capability in the enclosing snapshot; unavailable or disabled declarations may
name absent requirements.

Serde is implemented centrally rather than derived on each public value. Deserialization routes
back through constructors, rejects unknown object fields, requires already-reduced ratios, checks
cross-field invariants, and rejects noncanonical text. Signed and unsigned 64-bit wire integers are
decimal strings, which preserves exact values in JSON and JavaScript consumers. Maps and sets are
emitted in stable order. Containing project and protocol schemas must record or negotiate
`STABLE_PRIMITIVE_SCHEMA_REVISION` before decoding these values.

### Ownership and mutability

Most core values are immutable owned snapshots or `Copy` scalars. `ChannelLayout` owns a boxed
position slice. Settings, capabilities, discovery, diagnostic fields, and error fields own sorted
maps or sets. `Error` uniquely owns an optional boxed source chain. The principal shared mutable
type is `PerformanceCounter`, which owns an `AtomicU64`, uses relaxed ordering, saturates instead of
wrapping, and exposes only immutable snapshots for transport. Core owns no media bytes, GPU
resources, files, global registry, wall clock, process identity, or identifier allocator.

## Dependencies and consumers

### Direct dependencies

- Runtime: workspace `serde` with derive support, used to implement the stable wire contract.
- Test only: workspace `serde_json`, used to prove exact JSON forms, rejection, and round trips.
- Standard library: formatting, parsing, ordered collections, standard error sources, and atomics.
- No other Superi crate and no platform API is a dependency. This preserves the intended tier-zero
  dependency direction.

### Cargo-declared consumers

Eighteen workspace crates declare a direct path dependency on `superi-core`:
`superi-ai`, `superi-api`, `superi-audio`, `superi-cache`, `superi-cli`,
`superi-codecs-platform`, `superi-codecs-rs`, `superi-codecs-vendor`, `superi-color`,
`superi-concurrency`, `superi-effects`, `superi-engine`, `superi-gpu`, `superi-graph`,
`superi-image`, `superi-media-io`, `superi-project`, and `superi-timeline`.

Repository source search shows active direct Rust imports in twelve of them:

- `superi-api` uses shared errors and semantic versions for API behavior and version reporting.
- `superi-codecs-platform`, `superi-codecs-rs`, and `superi-codecs-vendor` use shared errors,
  identifiers, pixel and audio tags, color interpretation, and exact timing to implement platform,
  Rust, and external codec contracts.
- `superi-color` consumes color tags plus geometry, pixel, and error contracts while owning the
  actual transforms, gamut operations, working spaces, LUTs, HDR interpretation, and ICC paths.
- `superi-concurrency` uses job and media identifiers, exact clocks, diagnostic values, and the
  shared error model for pools, priorities, liveness, playback clocks, backpressure, and lifecycle.
- `superi-engine` uses shared errors, identifiers, color, pixel and alpha meaning, and exact
  timestamps at media, introspection, CPU-frame upload, cache identity, and foreground playback
  boundaries. Render-export additionally uses rational interval unions, exact rescaling, lifecycle
  errors, stream identity, and pixel or sample meaning to reject semantic drift before publication.
  The EngineControl error coordinator snapshots complete source chains and ordered contexts through
  `DiagnosticEvent::from_error`, maps explicit `Recoverability` into stable recovery intent, and
  exposes only `UserSafeError` plus reviewed user-safe fields at its presentation boundary.
- `superi-gpu` uses errors and diagnostics throughout resource and submission paths, plus color,
  pixel, and geometry values for conversion and upload contracts.
- `superi-graph` re-exports the official graph, node, port, edge, parameter, and resource identifier
  types so every graph-facing contract retains the same core-owned identity.
- `superi-image` uses color, geometry, identifiers, pixel and sample tags, time, timecode, and
  shared errors across image values, metadata, operations, previews, sequences, and I/O.
- `superi-media-io` is the broadest timing consumer. It uses identifiers, errors, color, pixel and
  sample tags, rational clocks, durations, ranges, rounding, and timecode across probe, demux,
  decode, container, VFR, image-sequence, PCM, selection, and metadata paths.
- `superi-timeline` uses project, media, timeline, track, clip, gap, transition, generator,
  caption, marker, and multicam angle identities with exact rational ranges to own validated
  editable project, annotation, synchronization, and switching state.

The other six declared consumers, `superi-ai`, `superi-audio`, `superi-cache`, `superi-cli`,
`superi-effects`, and `superi-project`, currently have no direct `superi_core::` reference in Rust
source. Their manifest dependencies express intended layering or scaffold readiness rather than
an exercised code relationship.

No external workspace source currently imports `superi_core::prelude`. The current downstream
contract is therefore coupled to the named module paths and to Serde trait implementations made
available by linking the crate. Changes to stable codes, field shapes, constructor validation,
time comparison, half-open semantics, or error classification can affect many crates even when no
consumer calls `serialization` directly.

## Invariants and operational boundaries

- Every official identifier is exactly one opaque `u128`, uses a permanent domain prefix and 32
  lowercase hexadecimal digits, and has platform-independent big-endian bytes. Core does not
  assign values or reserve zero.
- Stable tag codes are lowercase fixed strings. Unknown codes are rejected or return `None`;
  consumers of `non_exhaustive` enums must retain wildcard handling.
- Shared names are never trimmed, case-folded, or normalized. Settings and feature identifiers
  retain unknown vendor namespaces as valid canonical values.
- Ordered collections use `BTreeMap` or `BTreeSet`. Settings and feature snapshots reject duplicate
  entries; capability construction intentionally deduplicates repeated declarations.
- Floating-point geometry and diagnostic floating values must be finite. `FiniteF64` also
  normalizes negative zero to positive zero. Geometry retains ordinary `f64` equality semantics.
- All time rates are positive and reduced. Every rescale has an explicit rounding rule, duration is
  nonnegative, and interval ends are exclusive. Checked operations report shared user-correctable
  invalid-input errors rather than wrapping.
- Timecode never wraps at 24 hours. Drop-frame mode is supported only for denominator 1001 rates
  whose numerator is `nominal_fps * 1000` and whose nominal rate is a multiple of 30.
- Pixel formats describe storage, not allocation, orientation, alpha association, color meaning,
  or conversion support. Planar byte size remains a downstream stride and dimension concern.
- Channel layout equality, hashing, and ordering include the entire ordered sequence. Empty and
  repeated positions are invalid, including duplicate discrete positions.
- Color metadata is preserved exactly. Operational compatibility is intentionally not a core
  invariant.
- Error category and recoverability are independent. Raw summaries, contexts, source chains,
  diagnostic messages, internal fields, and sensitive fields are not user-safe by default.
- Field visibility is classification metadata, not encryption or automatic redaction. Full
  `DiagnosticEvent` serialization includes all fields and raw failure data. Only
  `UserSafeError` and `user_safe_fields` form the intended presentation boundary.
- Performance counters are monotonic within one instance, saturate at `u64::MAX`, never reset, and
  use relaxed atomics because they do not synchronize other state.
- Wire objects reject unknown fields and noncanonical representations. Existing codes, fields, and
  meanings are immutable within primitive schema revision 1; incompatible evolution requires
  revision negotiation or migration by the containing format.
- Public values are safe Rust. The module contains no `unsafe` block and performs no file, network,
  device, process, or GPU I/O.

## Tests and verification

The crate has 89 integration tests across 10 files:

- 7 diagnostic tests in `open/crates/superi-core/tests/diagnostics_contract.rs`.
- 7 shared-error tests in `open/crates/superi-core/tests/error_contract.rs`.
- 15 geometry tests in `open/crates/superi-core/tests/geometry_contract.rs`.
- 6 identifier tests in `open/crates/superi-core/tests/id_contract.rs`, including marker, media bin,
  smart collection, and multicam-angle domain identity, canonical text, parsing, and common traits.
- 9 media and color tag tests in `open/crates/superi-core/tests/media_tag_contract.rs`.
- 2 curated-prelude tests in `open/crates/superi-core/tests/prelude_contract.rs`.
- 7 stable-wire tests in `open/crates/superi-core/tests/serialization_contract.rs`, including the
  permanent `multicam-angle` code and typed text.
- 12 settings and discovery tests in `open/crates/superi-core/tests/settings_contract.rs`.
- 17 exact-time tests in `open/crates/superi-core/tests/time_contract.rs`.
- 7 timecode tests in `open/crates/superi-core/tests/timecode_contract.rs`.

Seven doc tests add positive examples for the prelude and timecode plus compile-fail proof that
points and vectors, identifier domains, and deliberately omitted prelude items cannot be confused.
There are no unit tests embedded in `src`; public integration contracts are the primary proof.

Fresh verification at mapping time ran:

```text
cargo test --manifest-path open/Cargo.toml -p superi-core --locked
```

All 89 integration tests and all 7 doc tests passed, with no failures or ignored tests. The suite
provides strong deterministic boundary coverage, including `i64` and `u64` extremes, malformed
wire values, source-chain privacy, atomics under threads, matrix horizons, and one-hour audio/video
alignment. It does not include fuzzing, property-based generation, cross-language fixtures, or
large hostile collection tests.

## Current status and risks

The module is implemented, actively consumed, and test-rich. Its crate-root `Status: skeleton`
documentation is stale and can mislead architecture readers or generated documentation.

Material risks and incomplete paths are:

- Primitive schema revision 1 is a broad compatibility surface. Changing an English
  `UserSafeError` title or action is wire-incompatible because deserialization verifies the exact
  derived text, not only its stable code and classification.
- Strict `deny_unknown_fields` decoding prevents silent truncation but also prevents forward field
  tolerance within one revision. Every additive object-field change needs an explicit compatibility
  decision rather than an assumed safe rollout.
- `DiagnosticEvent` carries raw messages, complete internal failure snapshots, and fields marked
  sensitive through serialization. A consumer that serializes or displays the whole event instead
  of selecting the user-safe projection can leak paths, identifiers, credentials, or media details.
- `ColorSpace::new` accepts every axis combination and `PixelFormat` exposes metadata only. This is
  deliberate for faithful ingest, but downstream code must never interpret construction success as
  proof that a codec, transform, GPU, or output supports the combination.
- `ChannelLayout` has no channel-count limit and checks duplicates with a quadratic scan. The wire
  boundary validates correctness but does not bound allocation or work for an untrusted very large
  layout.
- Matrix inversion rejects an exactly zero determinant and non-finite results, but it has no
  conditioning threshold. Numerically near-singular finite transforms can produce very large,
  unstable finite results that remain valid to the type.
- Serialization logic repeats permanent enum codes in macro invocations beside each type's
  `code` and `from_code` implementation. Current exhaustive matches and round-trip tests constrain
  drift, but new variants require coordinated edits in both locations and in the prelude decision.
- Stable wire coverage is JSON-proven through `serde_json`, not proven against other Serde formats
  or independent language implementations. Decimal wide integers and explicit shapes are designed
  for those consumers, but no checked-in cross-language fixture currently proves them.
- Parse and validation tests are representative rather than generated. Canonical name, semantic
  version, timecode, identifier, and decimal parsers have no fuzz corpus in this module.
- Six workspace crates declare the dependency without using a core type yet. Their intended
  ownership and integration paths remain incomplete outside this module, and their manifests alone
  should not be treated as consumer proof.

## Maintenance notes

Treat changes here as repository-wide public contract changes. Before altering a code, type shape,
constructor, parser, ordering rule, time formula, error category, recovery value, or wire field,
search every direct consumer and review project, API, engine, codec, media, image, color, GPU, and
concurrency behavior that depends on it.

When adding a shared type or enum variant, update its owning module, stable code lookup, Serde
implementation, schema-revision decision, curated-prelude decision, integration tests, and all
relevant downstream maps together. Do not automatically add every public item to the prelude.
Parse errors, raw diagnostics, and specialized boundary details are intentionally imported from
their owning modules.

Preserve representational separation. Do not fold alpha into pixel format, color transforms into
color tags, timecode labels into physical time, channel order into an unordered mask, identifier
allocation into the ID type, or buffer/resource ownership into core metadata. Those separations are
relied on by active codec, media, image, color, engine, GPU, and concurrency consumers.

For diagnostics, keep user-safe projection derived from stable classification only. Any new raw
field must receive an explicit visibility and must not be exposed through presentation helpers by
default. Review full-event transport separately because visibility does not redact serialization.

For wire evolution, reconcile compatibility before changing revision 1. Keep canonical integer
strings, reduced ratios, unknown-field policy, duplicate semantics, stable ordering, and checked
reconstruction aligned. Add cross-language fixtures if another language becomes a production
consumer.

After any owned source edit, rerun the core test suite, regenerate the module file inventory and
hash, inspect downstream use, and update this map's prose rather than changing only metadata.
