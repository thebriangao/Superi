---
module_id: superi-color
source_paths:
  - open/crates/superi-color
source_hash: 1ab9eeb47c6817da5eb4c3a8da97e4d27e0050b67b752f287a8d90e1b98d7a0a
source_files: 27
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-color` owns Superi's color-management math, canonical scene-linear image contract,
input color transforms, LUT parsing and evaluation, deterministic display, view, look, and output
rules, immutable versioned configuration, project working-space selections, ICC display-profile discovery state, and the
monitor-aware presentation guard around a native GPU viewport. It is the T3 color subsystem in
the open-tree dependency graph. It consumes platform-neutral color tags from `superi-core`, dense
CPU image artifacts from `superi-image`, and GPU frame and presentation ownership from
`superi-gpu`.

Implemented ownership is narrower than the full color architecture described by
`docs/phase-0-build-contracts.md`. Input and output transforms, transfer functions, primary
conversion, working-space storage, LUTs, ICC validation and discovery, and stale-profile
presentation checks are implemented. Managed GPU wide-gamut and native display transforms are
implemented for canonical `Rgba16Float` textures. The native display transform now includes exact
image, alpha, individual-channel, luminance, false-color, and display-linear clipping inspection.
Engine foreground playback now consumes the CPU output transform
for an explicit display branch. Engine render-export now invokes a separate caller-owned delivery
stage and validates its color history, but no concrete export transform in this crate is wired.
Executable ICC evaluation and concrete export conversion remain absent. The production shell now
consumes system display discovery, transactional profile catalogs, monitor bindings, and the GPU
display presenter through all four native viewer roles, with explicit sRGB and Display P3 output
choices plus one explicit transient analysis view per presentation.

The crate owns color interpretation, transform policy, and explicit legal-range RGB normalization,
but it does not own YUV matrix conversion, media decoding, image storage primitives, GPU device
creation, GPU command submission, native window creation, or graph scheduling. ICC transform evaluation is also not
implemented here yet: the ICC path currently owns profile discovery, validation, identity,
selection, and invalidation only.

## Source inventory

- `open/crates/superi-color/Cargo.toml`: crate manifest. It declares `serde`, `serde_json`, `sha2`, `superi-core`,
  `superi-gpu`, `superi-image`, and `superi-graph`, plus macOS-only Core Foundation and
  CoreGraphics bindings.
- `open/crates/superi-color/src/config.rs`: strict bounded JSON color-management files, immutable
  named scene-linear working spaces, aliases, roles, normalized SHA-256 semantic identity, and
  serializable project settings pinned to one exact configuration.
- `open/crates/superi-color/src/gamut.rs`: CIE xy colorimetry, RGB-to-XYZ matrix derivation,
  Bradford adaptation, wide-gamut RGB conversion, explicit negative-gamut policies, and working
  image conversion.
- `open/crates/superi-color/src/gpu_transform.rs`: managed compute-pipeline construction for
  reference-derived wide-gamut transforms, canonical texture validation, GPU-resident output
  allocation, pass encoding, submission, and fence-scoped ownership.
- `open/crates/superi-color/src/gpu_display.rs`: direct canonical-texture sampling into native or
  managed display attachments, shader constants derived from one `OutputColorTransform`, checked
  aspect fitting, sRGB attachment handling, deterministic scene-linear and display-linear analysis
  modes, and fence-scoped source retention.
- `open/crates/superi-color/src/hdr.rs`: validated relative-light, encoded-signal, normalized PQ,
  and nit value types; SDR, PQ, and HLG transfer functions; and complete reference HLG display
  rendering.
- `open/crates/superi-color/src/icc.rs`: bounded ICC v2 and v4 RGB display-profile parser,
  SHA-256 identity, display discovery interfaces, immutable snapshots, atomic catalog refresh,
  monitor bindings, and profile-change reporting.
- `open/crates/superi-color/src/icc/macos.rs`: macOS CoreGraphics active-display and ICC discovery
  boundary. This is the crate's only repository-owned unsafe implementation.
- `open/crates/superi-color/src/lib.rs`: crate documentation and public module declarations.
- `open/crates/superi-color/src/lut.rs`: strict bounded `.cube` parser, 1D and 3D LUT models,
  linear, trilinear, and tetrahedral interpolation, explicit domain policy, and application to
  promoted working images.
- `open/crates/superi-color/src/rules.rs`: validated immutable look, view, display, and delivery
  rules; explicit source-role filtering; first-applicable view selection; ordered LUT processing;
  and delegation to authoritative output transforms.
- `open/crates/superi-color/src/transform_in.rs`: explicit camera, display-referred, and
  scene-referred RGB input transforms into a selected working space.
- `open/crates/superi-color/src/transform_out.rs`: explicit working-to-display and
  working-to-deliverable RGB transform, including target validation, wide-gamut conversion,
  luminance-preserving tone mapping, premultiplied alpha handling, SDR, HLG, and PQ encoding,
  artifact preservation, and a separate legal-range RGB storage encoder.
- `open/crates/superi-color/src/view.rs`: profile-bound native viewport state, frame acquisition
  tokens, monitor move and profile refresh handling, and guarded GPU submission and presentation.
- `open/crates/superi-color/src/working_space.rs`: canonical scene-linear working-space,
  binary16 storage, binary32 computation, CPU image, and GPU descriptor contracts.
- `open/crates/superi-color/tests/color_fixture_contract.rs`: Consumes the versioned color
  baseline through a config-selected working space plus public image and transform interfaces and proves source and output intent,
  transfer order, HDR meaning, alpha association, wide-gamut round trips, and exact f16 and f32
  sample bits.
- `open/crates/superi-color/tests/config_contract.rs`: versioned schema, named spaces, aliases,
  roles, stable semantic hashing, project persistence and drift rejection, bounded real-file
  loading, strict malformed input, and sharing contracts.
- `open/crates/superi-color/tests/gamut_contract.rs`: reference primaries and matrix checks,
  adaptation, round trips, gamut policies, premultiplied alpha, metadata retention, and failure
  classification.
- `open/crates/superi-color/tests/gpu_transform_contract.rs`: constructor and validation contracts
  plus native upload, compute, submission, fence, explicit export readback, and binary64 CPU parity
  proof for canonical `Rgba16Float` frames.
- `open/crates/superi-color/tests/gpu_display_contract.rs`: frozen display metadata,
  resolution-independent aspect fitting, stable analysis identity, compatibility construction, and
  actual offscreen pixel parity for all eight analysis modes through the native presentation seam.
- `open/crates/superi-color/tests/icc_contract.rs`: ICC structure and tag validation, atomic
  discovery, monitor binding and viewport state, native provider behavior, macOS discovery, and
  unsafe-boundary inventory checks.
- `open/crates/superi-color/tests/input_transform_contract.rs`: input family semantics, transfer
  ordering, PQ reference white, HLG and ACES paths, alpha and metadata retention, binary16 range,
  and unsupported-input checks.
- `open/crates/superi-color/tests/lut_contract.rs`: `.cube` parsing, red-fastest 3D order,
  interpolation, parser limits, premultiplied working-image behavior, finite failures, and
  deterministic output.
- `open/crates/superi-color/tests/output_transform_contract.rs`: display and delivery target
  semantics, primary conversion before transfer encoding, SDR, HLG, and PQ behavior, metadata and
  window preservation, invalid configurations, physical PQ limits, and determinism.
- `open/crates/superi-color/tests/rules_contract.rs`: default and explicit view selection,
  source-role applicability, ordered look processing, independent delivery rules, artifact
  preservation, and fail-closed validation.
- `open/crates/superi-color/tests/transfer_contract.rs`: SDR reference anchors, PQ absolute
  luminance, HLG scene and display functions, round trips, domains, nonfinite failures, and sharing
  traits.
- `open/crates/superi-color/tests/working_space_contract.rs`: canonical ACEScg CPU and GPU
  descriptors, exact half payloads, promotion and quantization, invalid representations, and
  sharing traits.

## Public surface

`open/crates/superi-color/src/lib.rs` exposes twelve public modules: `config`, `gamut`, `gpu_display`, `gpu_transform`, `hdr`, `icc`,
`lut`, `rules`, `transform_in`, `transform_out`, `view`, and `working_space`. It does not re-export their
members at the crate root.

The working representation surface consists of:

- `WorkingSpace`, including `WorkingSpace::ACESCG`, validation from a complete `ColorSpace`, CPU
  `ImageDescriptor` creation, GPU `GpuFrameDescriptor` creation, and exact GPU descriptor
  validation.
- `WorkingImage`, the canonical `Rgba16Float` premultiplied image owner, with constructors,
  accessors, ownership release, and promotion to `WorkingImageF32`.
- `WorkingImageF32`, the distinct `Rgba32Float` computation owner, with constructors, accessors,
  ownership release, and binary16 quantization.

The gamut surface consists of `Chromaticity`, `RgbColorimetry`, `ChromaticAdaptation`,
`GamutMapping`, `LinearRgb`, and `WideGamutTransform`. The transform exposes its source and
destination definitions, selected policies, row-major binary64 matrix, destination CIE Y
coefficients, scalar RGB application, premultiplied RGBA application, and binary16 or binary32
working-image application.

The GPU transform surface consists of `GpuWideGamutTransform`, its encoded and submitted result
owners, and explicit encode and submit operations. Construction derives WGSL constants from the
binary64 CPU reference transform. Encoding validates canonical source texture shape and format,
allocates a canonical output texture, and returns an owned managed pass batch. Submission retains
the source, output, bindings, pipeline, and pass resources through the returned fence without an
ordinary CPU pixel path.

The GPU display surface consists of `GpuDisplayView`, `GpuDisplayPresenter`, `GpuDisplaySource`,
`EncodedGpuDisplayFrame`, and `DisplayViewport`. `GpuDisplayView` fixes the stable `image`, `alpha`,
`red`, `green`, `blue`, `luminance`, `false_color`, and `clipping` codes plus the source or display
linear-light stage they inspect. The compatibility presenter constructor selects image, while
`new_with_view` binds one explicit diagnostic interpretation, display transform, and attachment
format to the managed device lifetime. Encoding samples a canonical managed `Rgba16Float` texture
directly, applies reference-derived analysis plus the selected destination gamut and transfer
operations, aspect-fits arbitrary extents, and retains the sampled source until its presentation
fence retires.

The transfer surface consists of `RelativeLight`, `EncodedSignal`, `NormalizedSignal`, `Nits`, and
`HlgDisplayParameters`, plus `decode_relative_transfer`, `encode_relative_transfer`,
`convert_relative_transfer`, `pq_eotf`, `pq_inverse_eotf`, `hlg_oetf`, `hlg_inverse_oetf`,
`hlg_eotf`, and `hlg_inverse_eotf`. The distinct value types prevent relative scene light,
extended encoded signals, normalized PQ signals, and absolute display luminance from being mixed
without an explicit conversion.

The input surface consists of `InputSourceKind`, `InputTransformOptions`, and
`InputColorTransform`. Options expose Bradford or no chromatic adaptation, one explicit gamut
policy, and an optional PQ reference-white luminance. `apply_f32` produces promoted working
storage, while `apply` additionally enforces finite binary16 range before quantization.

The LUT surface consists of `DomainPolicy`, `LutInterpolation`, `Lut1D`, `Lut3D`, and `Lut`.
Callers can parse one strict `.cube` artifact, inspect its title, domain, size, and entries, apply
it to one RGB triplet, or apply it to a `WorkingImageF32`.

The ICC surface consists of the three public limits, `IccProfileId`, `IccVersion`,
`IccProfileClass`, `IccColorSpace`, `IccRenderingIntent`, `IccDisplayModel`, `IccTag`,
`IccProfile`, `MonitorId`, `DisplayProfileObservation`, `DisplayProfileDiscovery`,
`NativeDisplayProfileProvider`, the macOS-only `SystemDisplayProfileDiscovery`, `DisplayProfile`,
`DisplayProfileSnapshot`, `PresentationProfileState`, `MonitorPresentationBinding`,
`DisplayProfileUpdate`, and `DisplayProfileCatalog`.

The viewport surface consists of `ViewportProfileChange`, `MonitorAwareViewportState`,
`ViewportPresentationToken`, `MonitorAwareViewport`, and `MonitorAwareViewportFrame`. It wraps the
real `NativeViewportSurface` and exposes compatible-adapter discovery, configuration, monitor and
profile rebinding, guarded acquisition, the target texture, surface diagnostics, and guarded
submission plus presentation.

The output surface consists of `OutputTargetKind`, `ToneMapParameters`, `ToneMapping`,
`OutputTransformOptions`, `OutputColorTransform`, and `LegalRangeEncoder`. Options select chromatic
adaptation, explicit gamut mapping, an explicit luminance shoulder or no tone mapping, and an
optional PQ reference white. Construction binds one working space to one full-range RGB output
interpretation, while `apply` and `apply_f32` emit premultiplied RGBA binary32 `Image` artifacts.
`LegalRangeEncoder` is a separate downstream stage that exposes exact 8 through 16-bit RGB code
anchors and emits normalized, quantized, limited-range straight-alpha binary32 storage values.

The configuration surface consists of `ColorManagementConfig`, `ConfigWorkingSpace`, and
`ProjectColorSettings`. A config exposes its stable ID and normalized semantic fingerprint,
resolves canonical names or aliases, resolves roles, and selects a default scene-linear working
space. Project settings persist their schema version, config ID, fingerprint, and canonical
working-space ID, and resolve only against the exact config semantics they pin.

The rules surface consists of `SourceRole`, `ViewApplicability`, `LookRule`, `ViewRule`,
`DisplayRule`, `OutputRule`, and `ColorRuleSet`. Construction validates names, transform roles,
and look references. Selection retains explicit source semantics, and rendering applies named LUT
looks in declared order before the selected display or deliverable transform.

## Architecture and data flow

An input image reaches working space through a fixed semantic sequence. `InputColorTransform::new`
first requires explicit primaries and transfer tags, full-range RGB components, a supported source
family, and a compatible PQ option. Application then validates the image descriptor and packed
RGB or RGBA format, reads normalized integer samples or floating samples, derives or validates
alpha, unassociates premultiplied RGB, decodes the transfer function, converts primaries in
binary64, applies the selected gamut policy, reassociates alpha, and creates a canonical
premultiplied RGBA binary32 image. `apply` checks every result against the finite binary16 magnitude
limit and quantizes it; `apply_f32` leaves it in the distinct computation representation.

A working image reaches a display or deliverable through a second explicit sequence.
`OutputColorTransform::new` requires full-range RGB, explicit primaries and transfer, rejects a
linear display target, and requires a positive PQ reference white only for PQ deliverables.
Application validates the bound working space, unassociates nonzero premultiplied alpha, converts
linear primaries through `WideGamutTransform`, applies the selected destination-luminance shoulder,
encodes relative SDR or HLG light or absolute PQ luminance, reassociates alpha, and returns RGBA
binary32 with the destination color interpretation.
Windows, channels, source named-space and ICC payloads, and metadata are retained. The transform
does not itself apply a look, evaluate ICC tags, quantize to integer storage, or perform YUV matrix
conversion. The separate legal-range encoder unassociates full-range output RGB, rejects values
outside zero through one instead of clipping, rounds to the configured legal integer code, preserves
alpha, channels, windows, color payloads, and metadata, and tags the result limited range.

`ColorRuleSet` composes the existing operations without duplicating their math. A display chooses
the first source-applicable ordered view unless a compatible view is explicitly requested. Display
and delivery rules independently resolve ordered look names, apply each LUT in its declared working
process space, and then call the role-correct `OutputColorTransform`. Rule evaluation never mutates
the source working image or monitor presentation state.

Transfer ordering is deliberately split from numeric range and component-matrix conversion.
Relative SDR and scene HLG paths decode to `RelativeLight`. PQ decodes a normalized signal to
absolute nits and then divides by the caller's explicit working reference white. Display-referred
BT.709 and BT.2020 curves are rejected because the implemented functions are scene OETF inverses,
not the missing display EOTFs.

Primary conversion derives normalized RGB-to-XYZ matrices from published chromaticities. It
composes source conversion, optional Bradford reference-white adaptation, and inverse destination
conversion into one binary64 matrix. `GamutMapping::Preserve` retains all finite components,
`ClipNegative` clamps only negative values, and `PreserveLuminance` moves chroma toward the neutral
axis while preserving destination CIE Y. No policy clamps values above one or performs tone
mapping.

Canonical storage is a tagged, premultiplied, unqualified RGBA image in `Rgba16Float`.
Numerically sensitive work uses a separate `Rgba32Float` owner. Promotion and quantization preserve
windows, color tags, channel names, and metadata. They change only sample precision. GPU working
frames use one `Rgba16Float` texture plane with the same color and alpha interpretation.

The native GPU display path samples that canonical texture without mutation or readback. It
unassociates nonzero alpha in shader float computation. Image view retains the original black-backed
coverage; alpha and diagnostic views are opaque so inspected values are not multiplied twice.
Alpha, individual channels, source CIE Y, and fixed false-color exposure bands are formed in the
source scene-linear space before the unchanged gamut and transfer stages. Source CIE Y coefficients
are derived from the actual source primaries through `WideGamutTransform`. Clipping instead
classifies the configured display-linear result after primary and gamut conversion and before
transfer encoding or attachment clamping: under range is blue, over range is red, simultaneous
under and over range is magenta, and in-range output is destination-luminance gray.

Configuration loading reads at most 1 MiB plus one detection byte, parses one strict schema and
version, validates every declared space through `WorkingSpace`, canonicalizes aliases and role
targets, and hashes normalized semantics in deterministic map order. A project selection stores the
canonical space ID and exact config fingerprint, so JSON formatting changes do not break identity
while any color-semantic change prevents silent project reinterpretation.

LUT parsing accepts exactly one 1D or 3D declaration, optional title and domain directives, and a
complete finite table. 1D application interpolates channels independently. 3D application uses
the `.cube` red-fastest order and caller-selected trilinear or tetrahedral interpolation. Working
image application leaves zero-alpha pixel payloads bit-identical. For nonzero alpha it
unassociates, evaluates the LUT, reassociates, and replaces only the sample payload.

ICC bytes are treated as untrusted input. Parsing checks the total size, four-byte padding,
required header fields, v2 or v4 version, display class, RGB device space, XYZ or Lab connection
space, rendering intent, bounded and unique tag directory, exact tag ranges, reserved bytes,
contiguous padded tag elements, and one complete matrix/TRC or paired LUT display model. Only then
does it retain the bytes and derive the SHA-256 profile identity.

Display discovery is transactional. A provider returns one observation set, the catalog validates
all profiles and monitor constraints in temporary storage, sorts by monitor ID, computes changes,
and publishes a new immutable snapshot only if the entire refresh succeeds. Snapshot generation
increments on any semantic display-record change. A presentation binding captures that generation,
the monitor ID, and either the exact profile artifact or explicit unprofiled state.

The presentation path checks profile evidence twice. `MonitorAwareViewport` requires a current
binding before it acquires a real GPU frame. The returned frame carries a cloned presentation
token, and `submit_and_present` checks the current monitor and snapshot again before delegating to
the GPU submission queue and presenting. A monitor move, profile refresh, display-set change, or
mid-frame generation change therefore rejects stale presentation until the viewport is explicitly
rebound.

The production desktop composes the same lower contracts without introducing another color model.
It refreshes `SystemDisplayProfileDiscovery` into one shared `DisplayProfileCatalog`, creates a
fresh `MonitorPresentationBinding` for each selected viewer monitor, checks that binding before
surface acquire and before submit, and rebuilds `GpuDisplayPresenter` from one explicit
`OutputColorTransform` when the viewer chooses sRGB or Display P3. The application snapshot exposes
bounded profile metadata, exact content identity, freshness generation, working-space meaning,
precision, output transform ID, and canonical order, but never ICC bytes or frame pixels. During a
selection change the desktop hides that native child and reveals it only after a successful frame
through the replacement binding and transform. Both the native command and shell reply paths reject
older revisions, so rapid selections cannot republish or reveal an earlier transform. This is a real
built-in display-transform consumer, not an ICC tag evaluator.

## Dependencies and consumers

Direct runtime dependencies are:

- `superi-core` for `ColorSpace` axes, pixel and alpha formats, geometry, structured errors, and
  recoverability.
- `superi-image` for immutable CPU `Image`, `ImageDescriptor`, `ImageSamples`, color payloads,
  channels, and metadata.
- `superi-gpu` for working-frame descriptors, texture formats, GPU devices and instances, native
  viewport surfaces, acquired frames, submission resources, and fences.
- `sha2` for complete ICC-profile content identity.
- `serde` and `serde_json` for strict configuration input and stable project-settings persistence.
- macOS-only `objc2-core-graphics` and `objc2-core-foundation` framework bindings for active
  display and profile discovery.

`superi-graph` is declared in `open/crates/superi-color/Cargo.toml`, but no current crate source
uses it. This keeps graph integration conceptual rather than implemented. The architecture rule in
`open/docs/STRUCTURE.md` still matters: `superi-graph` must not depend upward on `superi-color`;
node catalogs or orchestration must consume both from above.

`superi-engine::playback` is the first runtime source consumer. Its
`CpuPlaybackDisplayTransform` binds one exact nonterminal scene pipeline and display stage to
`OutputColorTransform`, executes the transform on a playback-priority worker, and publishes the
result with `ViewportColorMetadata` through a bounded handoff. Native surface submission remains a
separate desktop-owned consumer, while ICC evaluation and concrete export remain unwired. Engine
`export_queue` owns a generic `ExportVideoDelivery<V>` seam and validates the returned terminal
output pipeline, scene continuity, format, and alpha meaning, but it does not import or execute a
`superi-color` transform. The crate's own public
integration contracts, including the canonical repository color fixture, remain the direct color
algorithm proofs. The fixture contract reads versioned artifacts directly and does not add a
runtime dependency on the repository fixture generator.

The retired presentation consumer has been removed. `superi-ui` currently paints native application
chrome but does not yet consume canonical viewer textures or `GpuDisplayPresenter`. A future
retained viewer must compose the existing system catalog, exact monitor profile binding, analysis
view, and built-in sRGB or Display P3 intent above the sole managed GPU submission owner. Selected
and last-presented analysis must remain distinct from playback, navigation, overlays, comparison,
status, and frame ownership.

`docs/unsafe-ffi.md` consumes the macOS boundary as an audit inventory, and
`open/crates/superi-color/tests/icc_contract.rs` verifies that this inventory remains present.

## Invariants and operational boundaries

- A `WorkingSpace` must use explicit BT.2020, Display P3, ACES AP0, or ACES AP1 primaries with
  linear transfer, RGB matrix, and full range. ACEScg is the default.
- Color config files are capped at 1 MiB and 64 working spaces. Schema versions, unknown fields,
  nonlinear or limited-range spaces, duplicate names or aliases, missing role targets, and config
  drift fail closed. Project identity uses validated semantics, not JSON formatting.
- Canonical CPU and GPU storage is premultiplied `Rgba16Float`. Binary32 is a distinct computation
  representation and cannot be constructed as canonical storage.
- GPU analysis never changes the canonical texture or scene metadata. Image view preserves source
  coverage, diagnostic views are opaque, zero-alpha RGB avoids division, source inspection precedes
  the output transform, and clipping alone observes display-linear values before transfer encoding.
- Working images require exactly the unqualified `R`, `G`, `B`, `A` channel order. Construction
  validates representation and interpretation but intentionally does not clamp sample payloads.
- Input transforms require prior full-range RGB conversion. They do not silently choose YUV
  matrices, legal-range scaling, source family, transfer function, PQ reference white, chromatic
  adaptation, or gamut policy.
- Output transforms likewise emit full-range RGB and require callers to choose target kind,
  destination interpretation, chromatic adaptation, gamut policy, tone mapping, and PQ reference
  white. They do not silently perform looks, ICC evaluation, YUV conversion, legal-range packing,
  or integer quantization. Tone mapping defaults to none and uses one RGB scale factor when enabled.
- Legal-range RGB encoding is explicitly downstream of the full-range output transform. It requires
  premultiplied RGBA binary32 RGB, preserves alpha, rejects extended values, and never chooses a YUV
  matrix or hides integer code rounding.
- Rule names and look references are validated at construction. Display views accept only display
  transforms, delivery rules accept only deliverable transforms, explicit inapplicable views fail,
  and look process spaces must match the working image.
- Premultiplied input alpha must be finite and within zero through one. Zero-alpha premultiplied
  input must have zero RGB. Gamut application rejects negative or nonfinite alpha.
- Primary and transfer calculations reject nonfinite inputs and successful nonfinite results.
  Values above one and, where the chosen semantic domain permits them, negative values remain
  explicit.
- PQ is bounded to normalized signals and zero through 10,000 nits. HLG scene encoding rejects
  negative light but preserves production headroom above one.
- LUT source is capped at 128 MiB, 1D tables at 65,536 entries, and 3D tables at 2,000,000 entries.
  Out-of-domain behavior is always selected by the caller.
- ICC profiles are capped at 16 MiB and 4,096 tags. Display snapshots are capped at 64 active
  monitors, reject duplicate IDs and multiple primaries, and never guess a missing profile.
- A failed display refresh leaves the prior snapshot unchanged. Bindings use exact profile content
  identity, not display names or an assumed standard profile.
- Native presentation must use a current monitor binding at both acquisition and presentation.
  GPU surface ownership, device matching, queue ownership, and resource retirement remain enforced
  by `superi-gpu`.
- `open/crates/superi-color/src/icc/macos.rs` locally permits unsafe code for two
  `CGGetActiveDisplayList` calls. The count query uses a null list only with zero capacity. The fill
  uses an exactly sized initialized buffer, and a confirmation query rejects a display-set race.
  No raw CoreGraphics handle leaves the module.

## Tests and verification

The twelve integration suites cover the implemented CPU, GPU display, and presentation-state contracts:

- `open/crates/superi-color/tests/color_fixture_contract.rs` checks all eight canonical SDR,
  wide-gamut, PQ, HLG, alpha, f16, and f32 images through explicit input and output intent. It also
  verifies contiguous payload offsets, per-image SHA-256, exact source bits, and authoritative
  output tags.

- `open/crates/superi-color/tests/config_contract.rs` checks strict versioned parsing, deterministic
  semantic identity, named spaces, aliases, roles, bounded disk loading, stable project serialization,
  config-drift rejection, invalid inputs, and concurrency traits.

- `open/crates/superi-color/tests/working_space_contract.rs` proves canonical descriptors, exact
  half payload retention, promotion and quantization, and rejection of mislabeled storage.
- `open/crates/superi-color/tests/transfer_contract.rs` checks standards anchors, extended SDR
  round trips, PQ precision, HLG rendering, domain failures, and nonfinite containment.
- `open/crates/superi-color/tests/gamut_contract.rs` checks published primaries, the ACES AP0 to AP1
  reference matrix, Bradford behavior, negative and HDR round trips, gamut policy, and artifact
  retention.
- `open/crates/superi-color/tests/gpu_transform_contract.rs` checks constructor metadata, fail-closed
  source validation, native compute submission and fence ordering, explicit export readback, and
  per-channel parity with the binary64 CPU reference after source half quantization.
- `open/crates/superi-color/tests/gpu_display_contract.rs` checks centered 8K aspect fitting,
  zero-extent rejection, canonical GPU source format, the exact display reference, compatibility
  image construction, stable view codes and stages, and actual per-channel offscreen parity for all
  eight modes after RGBA16F source quantization.
- `open/crates/superi-color/tests/input_transform_contract.rs` proves source-family distinctions,
  decode-before-primary-conversion order, explicit PQ reference white, canonical output, and
  binary16 overflow rejection.
- `open/crates/superi-color/tests/lut_contract.rs` covers strict parsing, all interpolation modes,
  red-fastest storage, bounds, premultiplied application, and deterministic results.
- `open/crates/superi-color/tests/output_transform_contract.rs` covers display and delivery target
  validation, canonical and promoted working inputs, primary conversion and tone mapping before
  encoding, SDR, HLG, absolute PQ, exact legal-range anchors and stage separation, alpha and artifact
  preservation, physical-domain failures, and determinism.
- `open/crates/superi-color/tests/rules_contract.rs` covers ordered default selection, explicit
  applicability, real LUT ordering before encoding, independent delivery selection, metadata and
  window preservation, transform-role validation, missing references, and process-space failures.
- `open/crates/superi-color/tests/icc_contract.rs` covers ICC parser limits and models,
  transactional catalog state, stale monitor tokens, shell-provided native IDs, audit inventory,
  and macOS discovery when a display server is available.

The engine playback contract constructs decoded provenance, a canonical working image, one CPU
display transform, and a bounded viewport payload. The engine render-export contract separately
proves delivery-pipeline and alpha validation through a caller-owned test stage, but it does not
exercise this crate's output transform. The desktop application now exercises native GPU viewport
presentation through exact active-monitor bindings and both built-in display transforms, while
project-configured looks, arbitrary ICC evaluation, and concrete color-to-export proof remain absent.

## Current status and risks

The configuration, working-space, gamut, transfer, input, output, LUT, ICC state, and profile-guarded viewport
contracts are implemented and extensively tested. The module is not yet a complete color pipeline.

- Versioned named scene-linear spaces, aliases, roles, and project pinning are implemented. File
  rules, config-persisted looks, displays, views, context variables, and transform graphs do not
  exist in this configuration schema; runtime look and output rules remain separate typed APIs.
- Output transforms and rule evaluation remain CPU implementations that emit RGBA binary32
  artifacts. Engine foreground playback is a concrete display consumer, but executable ICC profile
  evaluation, project-configured rule persistence, concrete integer or YUV encoding, complete
  HDR display and delivery GPU output transforms, and concrete export conversion remain absent. The
  implemented native presenter supports the current SDR sRGB and Display P3 built-in slices plus
  their eight deterministic inspection modes.
- ICC profiles are validated and bound to presentation, but their matrix/TRC or LUT payloads are
  not evaluated. `MonitorAwareViewport` prevents stale profile use but does not color-convert the
  rendered texture by itself.
- Engine foreground playback is a live CPU output-transform consumer. All four desktop viewer roles
  are native GPU display consumers with independent exact monitor selection. Engine export has only
  a generic delivery seam, not a live
  transform from this crate, and viewer-specific render-result binding remains separate.
- `superi-graph` is an unused manifest dependency. No color node catalog or graph-visible transform
  integration exists in this crate.
- CPU input, gamut, and LUT application allocate replacement sample vectors and iterate pixels on
  the CPU. The managed GPU path currently covers wide-gamut linear transforms only; it has no
  graph consumer and no cross-transform compiled cache. The engine CPU playback path does not claim
  GPU output execution.
- `WorkingImageF32::quantize_f16` directly narrows samples and can encode finite values beyond the
  binary16 maximum as infinity. `InputColorTransform::apply` guards this, but direct callers of
  quantization must own that semantic choice. Canonical working construction also intentionally
  accepts NaN and infinity payloads.
- LUT application validates only that nonzero alpha is finite. Unlike input and gamut paths, it does
  not reject a negative nonzero working alpha, so callers must maintain the working-alpha
  invariant before LUT evaluation.
- Any display-record change increments the global snapshot generation, which conservatively makes
  every existing binding stale, even for a monitor whose own profile did not change. Conversely,
  `DisplayProfileUpdate::profile_changed` reports profile identity changes only; name, primary, or
  built-in changes affect `changed` and generation but do not appear in that list.
- The accepted ICC subset requires padded tag elements to be contiguous after the tag table. ICC
  files outside that strict structural subset fail even if another ICC consumer might accept them.

## Maintenance notes

After any source change under `open/crates/superi-color`, rerun the mapping script's `files` and
`hash` commands, update both metadata and prose, and run the complete contract suite. Any new source
file must appear in the inventory.

Changes to canonical image meaning must be reconciled with `superi-core` color and pixel tags,
`superi-image` descriptors, and `superi-gpu` frame descriptors. Changes to viewport ownership must
be reconciled with `superi-gpu` surface and submission lifetimes plus the desktop catalog, binding,
selection, and diagnostics consumer. Graph integration must preserve
the downward dependency rule in `open/docs/STRUCTURE.md`.

Changes to `open/crates/superi-color/src/icc/macos.rs`, its target dependencies, or any new unsafe
boundary require a matching update to `docs/unsafe-ffi.md` and target-specific Clippy proof. Keep
profile absence explicit and retain atomic snapshot publication.

When configuration becomes real, replace the placeholder in
`open/crates/superi-color/src/config.rs`, add its complete contracts and tests, and update the
dependency and consumer trace. Keep the existing engine CPU playback consumer and its bounded
viewport integration explicit, and keep the engine export delivery seam distinct from a concrete
color transform. Preserve the current native sRGB and Display P3 consumer, its exact profile
freshness checks, and its explicit limitation to built-in transforms. Do not describe future OCIO,
arbitrary ICC evaluation, broader GPU output conversion, or concrete color export conversion as
implemented before the source and end-to-end consumers exist. Keep legal-range RGB encoding
distinct from later YUV matrix and packed integer storage ownership.
