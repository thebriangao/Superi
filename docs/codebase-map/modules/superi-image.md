---
module_id: superi-image
source_paths:
  - open/crates/superi-image
source_hash: 35e2b0bece0bb57ea678dfa181e89e6b39fadc0acc4151a70ec8621fb41dab65
source_files: 26
mapped_at_commit: 217e9d48703bcfd4736d949aea510c94505071bc
---

## Purpose and ownership

`superi-image` is the host-side image representation, still-image interchange, deterministic CPU
processing, and reference-validation substrate. It composes geometry, pixel, color, timing, media
identity, and structured error types from `superi-core` into immutable image artifacts. It owns
exact ordered channel and layer identity, alpha association, typed metadata, distinct signed data
and display windows, bounded allocation policy, native byte storage, dense CPU values, tiled and
mip access, still-image and numbered-sequence I/O, CPU image operations, and isolated thumbnail and
waveform products.

The crate deliberately keeps three image representations separate:

- `Image` is a tightly packed, row-major dense CPU value. All channels use one scalar
  representation and a packed `PixelFormat`.
- `ImageStorage` and `ImageAccess` expose immutable host bytes with explicit planes, strides,
  channel slices, per-channel sample types, scanline or tiled organization, mip levels, signed
  coordinates, metadata, and optional sequence identity. Access does not convert or repack bytes.
- `StillImage` is a nonempty ordered multipart container whose named parts each hold one
  `ImageAccess`. It can retain private format-native source state for exact same-format export.

These values are not interchangeable. Conversion from codec-native storage to a dense processing
image, or from either host representation to a GPU texture, requires an explicit owner outside the
implicit access APIs. `superi-image` contains no GPU device, texture, queue, shader, command, fence,
or residency object. `superi-color` owns actual color transforms over dense `Image` values, and
`superi-gpu` owns GPU-resident frames and explicit upload, conversion, and readback.

## Source inventory

- `open/crates/superi-image/Cargo.toml`: Declares the crate and its direct dependencies on
  `superi-core`, `half`, `exr`, `image`, and `image-webp`.
- `open/crates/superi-image/src/alpha.rs`: Resolves recognized color-to-alpha bindings and performs
  bounded opaque, straight, and premultiplied transforms over logical pixels and dense images.
- `open/crates/superi-image/src/channels.rs`: Owns exact channel names, nested layer names,
  conventional channel recognition, stable indices, and validated ordered channel lists.
- `open/crates/superi-image/src/io.rs`: Implements bounded still-image read and
  representation-aware write for EXR, DPX, PNG, JPEG, TIFF, WebP, TGA, and BMP.
- `open/crates/superi-image/src/lib.rs`: Documents the crate boundary and publicly exposes all
  twelve implementation modules without a facade re-export layer.
- `open/crates/superi-image/src/limits.rs`: Defines finite dimension, allocation, channel, layer,
  metadata, and tile limits plus checked allocation and clone helpers.
- `open/crates/superi-image/src/metadata.rs`: Defines orientation, exact metadata floats, typed
  metadata values, authoritative color tags, ICC and named-space payload retention, pixel aspect,
  timecode, and deterministic attributes.
- `open/crates/superi-image/src/model.rs`: Defines immutable interleaved or planar CPU byte storage,
  including plane origins, row strides, relative alignment, channel slices, and checked addressing.
- `open/crates/superi-image/src/ops.rs`: Implements dense CPU crop, resize, affine transform, flip,
  quarter rotation, blend, source-over composite, channel remap, and channel rename.
- `open/crates/superi-image/src/preview.rs`: Implements aspect-fit thumbnails and deterministic
  waveform-envelope rasterization into dense UI images.
- `open/crates/superi-image/src/reference.rs`: Dispatches canonical CPU reference operations and
  compares dense reference and candidate images with bounded mismatch reports.
- `open/crates/superi-image/src/sequence.rs`: Implements filesystem sequence pattern parsing,
  discovery, missing-frame resolution, still-image reading, semantic-black synthesis, and
  collision-safe sequential writing.
- `open/crates/superi-image/src/tiling.rs`: Defines scanline, tile, mipmap, region, and native-byte
  access semantics with complete tile-set validation.
- `open/crates/superi-image/src/value.rs`: Defines exact sample payloads, packed dense descriptors,
  immutable dense images, metadata attachment, and per-image scalar representation.
- `open/crates/superi-image/tests/alpha_contract.rs`: Proves alpha lookup, conversion rules,
  rounding, payload preservation, dense conversion, and error reporting.
- `open/crates/superi-image/tests/channel_naming_contract.rs`: Proves exact naming, nested layers,
  recognition, ordering, Unicode and case behavior, lookup, and duplicate rejection.
- `open/crates/superi-image/tests/cpu_reference_contract.rs`: Proves deterministic reference
  dispatch, exact and tolerant comparison, IEEE special-value behavior, and mismatch diagnostics.
- `open/crates/superi-image/tests/image_access_contract.rs`: Proves scanline, tile, mip, region,
  signed-coordinate, native-precision, and sequence-position access contracts.
- `open/crates/superi-image/tests/image_model_contract.rs`: Proves dense descriptor and sample
  identity, packed channel order, exact integer and float payloads, metadata edits, and rejection.
- `open/crates/superi-image/tests/image_ops_contract.rs`: Proves spatial, blend, composite, channel
  remap, channel rename, alpha-aware filtering, extent, metadata ownership, and error behavior.
- `open/crates/superi-image/tests/image_sequence_contract.rs`: Proves pattern parsing, deterministic
  discovery, numbering, missing policies, concrete I/O, black synthesis, no-clobber publication,
  cleanup, and retry.
- `open/crates/superi-image/tests/metadata_contract.rs`: Proves orientation, optional defaults,
  aspect, timecode, deterministic typed attributes, exact payloads, and color-tag separation.
- `open/crates/superi-image/tests/preview_contract.rs`: Proves thumbnail fit and filtering plus
  timed, channel-separated waveform raster output and validation.
- `open/crates/superi-image/tests/resource_limits_contract.rs`: Proves configurable finite limits,
  allocation preflight, decode bounds, and panic resistance for selected malformed inputs.
- `open/crates/superi-image/tests/still_image_io_contract.rs`: Proves eight-format dispatch and
  representative raster, multipart EXR, tiled mip, metadata, and packed DPX round trips.
- `open/crates/superi-image/tests/storage_contract.rs`: Proves immutable planes, interleaved and
  planar layouts, origins, strides, alignment, channel slices, bounds, sharing, and overflow checks.

## Public surface

The crate root exposes `alpha`, `channels`, `io`, `limits`, `metadata`, `model`, `ops`, `preview`,
`reference`, `sequence`, `tiling`, and `value` as public modules.

Channel and alpha contracts:

- `ChannelName` preserves exact UTF-8 spelling and case. `LayerName` validates dot-delimited nested
  components, while malformed dot patterns remain lossless unqualified channel names.
  `StandardChannel` recognizes exact base meanings such as `R`, `G`, `B`, `A`, `Y`, component
  alpha, depth, and ID without rewriting the source name.
- `ChannelList` is nonempty, ordered, exact-name unique, and the authority for stable
  `ChannelIndex` values. It resolves requested names in request order and derives layers
  deterministically, with the base first and ancestors before descendants.
- `AlphaLayout` binds `R`, `G`, `B`, and `Y` to component, enclosing-layer, or base alpha channels.
  `AlphaTransform` converts logical F32, normalized U8/U16, raw F16/F32 payload buffers, or dense
  `Image` values. `PremultiplicationRule::PreserveZeroAlpha` is restricted to temporary
  straight-to-premultiplied reassociation.

Dense value and storage contracts:

- `ImageSampleType` describes U8, U16, U32, F16, and F32 native semantics. `ImageSamples` can own
  U8, U16, F16-bit, or F32-bit dense payloads; dense U32 is intentionally not implemented.
- `ImageDescriptor` combines independent signed data and display windows, one supported packed
  `PixelFormat`, exact channels, `ImageColorTags`, `AlphaMode`, and a uniform sample type.
  `Image` adds exact immutable samples and typed `ImageMetadata`; construction validates
  representation and complete sample count.
- `StoragePlane`, `ChannelSlice`, `ChannelStorageLayout`, and `ImageStorage` describe immutable
  `Arc<[u8]>` host storage. Addresses include plane origin, row and pixel stride, channel byte
  offset, and sample width. `ByteAlignment` guarantees relative row offsets, not the backing
  allocation address needed for aligned typed dereference.
- `ImageAccessDescriptor` retains exact channels, a sample type per channel, color and alpha tags,
  metadata, windows, and optional `ImageSequencePosition`. `ImageAccess` is either one complete
  scanline storage value or a complete validated tile set with `TileDescription`, `MipMode`,
  `LevelRoundingMode`, `MipLevel`, `TileIndex`, and `ImageTile` identity. Borrowed regions and rows
  return native sample bytes without stitching, filling, conversion, or allocation.

Metadata and resource contracts:

- `ImageOrientation` preserves TIFF/Exif values 1 through 8 without applying them to pixels.
  `ImageMetadataFloat` retains exact binary64 bits. `ImageMetadataValue` supports Boolean, text,
  signed and unsigned integers, exact floats, and shared uninterpreted bytes.
- `ImageColorTags` keeps one authoritative `ColorSpace` separate from optional named source-space
  text and ICC bytes. Those source payloads do not override the interpretation or select a
  transform. `ImageMetadata` separately retains optional orientation, aspect, timecode, and
  deterministically ordered arbitrary attributes.
- `ImageLimits` defaults to 65,536 by 65,536 pixels, 512 MiB per checked allocation or result,
  1,024 channels, 64 layers, 16 MiB metadata, and 1,048,576 tiles. Builders can replace each
  maximum with a nonzero tighter or broader policy.

Processing and validation contracts:

- Dense operations expose nearest and bilinear resampling, display-relative flips and quarter
  turns, affine transformation, crop, blend, source-over composition, and channel mapping. Every
  allocating operation has a default finite-limit entry point and an explicit `_with_limits`
  form. Exact spatial and copy operations retain raw payloads; arithmetic paths decode, compute,
  and deterministically re-encode the declared sample representation.
- `UnaryReferenceOperation` and `BinaryReferenceOperation` package CPU operations as immutable
  requests. `compare_images` checks descriptor and metadata identity before deterministic sample
  comparison. `ReferenceTolerance` is exact or absolute; integers remain exact, while tolerant
  float comparison has explicit NaN and infinity handling. `ReferenceComparison` records counts,
  maximum defined error, and only the first `SampleMismatch`.
- `ThumbnailRequest` and `generate_thumbnail` produce an aspect-fit dense CPU image without
  upscaling. `WaveformEnvelope`, `WaveformPeak`, and `WaveformRasterStyle` produce a timed,
  channel-separated RGBA8 sRGB `WaveformImage` that retains the source envelope beside the raster.

Still and sequence I/O contracts:

- `StillImageFormat` recognizes EXR, DPX, PNG, JPEG, TIFF, WebP, TGA, and BMP plus stable extension
  aliases. `ReadOptions` supplies limits and optional requested sequence identity. `WriteOptions`
  controls JPEG quality and DPX endianness, packing, and bit depth.
- `StillImageLayer` pairs an optional unique part name with one `ImageAccess`. `StillImage` is a
  nonempty ordered multipart value. `read`, `write`, `read_path`, and `write_path` are the stream
  and path entry points; stream I/O requires seekability and read buffering.
- `ImageSequencePattern` parses canonical signed frame labels with explicit minimum padding.
  `ImageSequenceManifest` is a deterministic discovery snapshot with separate zero-based logical
  image number and signed file-frame number. `ImageSequenceReader` applies `Error`, `Hold`, or
  `Black` missing-frame policy. `ImageSequenceWriter` publishes sequential still files without
  replacement and advances only after successful publication.

## Architecture and data flow

Dense CPU processing starts with `ImageDescriptor + ImageSamples + ImageMetadata`, validates a
packed immutable `Image`, dispatches through `ops` or `AlphaTransform`, and returns a new validated
`Image`. Crop, nearest resize, flips, quarter turns, channel copies, and endpoint blend paths retain
exact source payload bits where their contract permits. Bilinear resize, general affine sampling,
blend, source-over, and alpha math decode integer or floating samples, work in associated-alpha
space as needed, and re-encode deterministically. Operations preserve color tags rather than
transforming color, and they do not apply orientation metadata.

The native import path is file bytes to a bounded decoder, then exact channels, metadata, signed
windows, sample types, and host planes, then validated scanline or tiled `ImageAccess`, then one or
more `StillImageLayer` values. The native access path stays distinct from dense processing. Region
and scanline views select source channels and coordinates while returning original sample bytes.

Raster import uses the `image` crate for PNG, JPEG, TIFF, WebP, TGA, and BMP. It maps native
supported decoder output to canonical `Y`, `YA`, `RGB`, or `RGBA` interleaved storage, marks decoded
color as sRGB, marks alpha as straight when present, retains selected ICC, Exif, orientation, source
color-type, and sequence metadata, then stores bounded original file bytes. Reconstructed raster
export requires one scanline level, origin-zero equal data and display windows, canonical channel
sets, uniform supported precision, representable metadata and color state, and compatible opaque or
straight alpha. It performs no color, alpha, precision, orientation, resize, or channel conversion.
When format and options match, a decoded image can instead write the retained original bytes
verbatim.

EXR import first preflights metadata and structural budgets, then decodes all supported flat parts,
channels, layers, and levels in pedantic non-parallel mode. It preserves F16, F32, and U32 payloads,
signed windows, part and channel names, scanline or tiled mip organization, and mapped attributes.
Color interpretation remains unspecified. A final-component `A` channel marks a part as
premultiplied. Exact original EXR state is retained for verbatim output. Reconstructed EXR supports
flat F16/F32/U32 scanline or tiled mip data, a shared display window and pixel aspect, EXR
premultiplied-alpha convention, and the implemented metadata namespaces. It rejects deep data,
subsampled channels, ripmaps, typed timecode output, and declared color meaning until a faithful
chromaticities mapping exists.

DPX import parses one uncompressed image element from the fixed header and supports luma, RGB,
RGBA, or ABGR at 8, 10, 12, or 16 bits in big- or little-endian form with supported filled packing.
It validates dimensions, offsets, padding, row and file ranges before unpacking into canonical
planar host storage, preserves source and sequence metadata, leaves color unspecified, and treats
RGBA as straight. Reconstructed DPX writes one scanline level with supported canonical channels,
integer precision, nonnegative offsets, representable windows, compatible alpha, and no unmodeled
color declaration. It rejects values above the selected bit depth rather than truncating them.

Sequence import scans a directory into a deterministic immutable manifest, resolves an explicit
missing policy, attaches the requested `ImageSequencePosition`, and calls still-image I/O. `Hold`
uses the nearest earlier available frame but retains requested logical identity. `Black` decodes a
reference solely for complete representation, then rebuilds every part, plane, tile, and mip with
zeroed native bytes and numeric one in recognized alpha channels. Sequence export infers one still
format, writes to a unique same-directory temporary file, flushes and synchronizes it, and publishes
with a no-replacement hard link.

Preview and validation paths are intentionally isolated. CPU-visible `Image` to thumbnail is an
aspect-fit transform, and decoded audio in `superi-media-io` becomes peak columns, then a validated
`WaveformEnvelope`, then an RGBA8 CPU raster. A GPU owner must explicitly read back a texture before
calling thumbnail code. GPU validation similarly materializes a CPU candidate first, then compares
it with a `superi-image` CPU reference; the reference layer never reads a texture or selects a
production fallback.

## Dependencies and consumers

Direct crate dependencies are:

- `superi-core` for shared errors, geometry, matrices, bounds, pixel formats, alpha modes, color
  spaces, aspect ratio, timecode, sample time, channel layout, and media identity.
- `half` for IEEE binary16 conversion used by alpha, dense operations, previews, references, and
  semantic-black synthesis.
- `exr` for flat multipart EXR metadata, scanline, tile, mip, and sample import and export.
- `image` and `image-webp` for conventional raster decode and encode paths.

Internal direction is from semantic primitives (`channels`, `metadata`, `limits`, `model`, `value`)
to processing (`alpha`, `ops`, `reference`, `preview`), native access (`tiling`), and I/O
orchestration (`io`, `sequence`). `io` composes channel, limit, metadata, storage, tiling, and sample
contracts. `sequence` composes `io`, storage, tiling, channels, limits, and sequence identity.

Current source-level consumers are narrower than the manifest graph:

- `superi-color` consumes dense `Image`, `ImageDescriptor`, and `ImageSamples`. Its working-space,
  input-transform, gamut, and LUT paths validate or rebuild exact dense artifacts, preserve windows
  and metadata, and explicitly change authoritative color interpretation when a real transform is
  applied. Canonical working images are premultiplied RGBA F16, with distinct F32 computation
  images. This confirms color transformation belongs above this crate.
- `superi-gpu` test contracts consume `PremultiplicationRule`, `UnaryReferenceOperation`,
  `compare_images`, `ReferenceTolerance`, and dense image types to compare an explicitly read-back
  GPU alpha conversion with the CPU oracle. GPU frames, uploads, textures, conversion plans,
  commands, submissions, and residency remain owned by `superi-gpu`; no implicit host/GPU bridge
  originates here.
- `superi-media-io` consumes `WaveformEnvelope`, `WaveformPeak`, `WaveformRasterStyle`,
  `WaveformImage`, and `render_waveform_image`. It owns decoded PCM validation and peak extraction,
  then delegates validated rasterization to this crate.

`superi-ai`, `superi-cache`, `superi-codecs-platform`, `superi-codecs-rs`,
`superi-codecs-vendor`, `superi-effects`, `superi-engine`, and `superi-graph` also declare
`superi-image` dependencies. No direct `superi_image` imports were found in their current Rust
sources during synthesis, so these are declared integration, scaffold, or transitive boundaries,
not evidence of an implemented direct runtime consumer. `superi-color`, `superi-gpu`, and
`superi-media-io` are both manifest and source consumers.

## Invariants and operational boundaries

- Ordered full channel names, nested-layer identity, sample type, plane layout, windows, alpha
  mode, color interpretation, metadata, multipart identity, mip identity, and optional sequence
  position are first-class properties. Code must not infer channel meaning solely from position or
  collapse signed data and display windows into origin-zero dimensions.
- Dense samples are tightly packed in row-major pixel order and exact channel order, with one
  representation matching the descriptor. Native access can be planar or interleaved, mixed
  precision, tiled, and mipmapped. A tiled access is complete and eager, not sparse, virtual, or
  deferred, and every tile shares one physical layout contract.
- Floating payload storage is bit-preserving until an explicit arithmetic operation decodes and
  re-encodes it. Preservation paths retain integer values and IEEE payloads exactly. Numeric paths
  use deterministic normalized-integer rounding and explicit F16/F32 encoding.
- Alpha transforms touch only recognized color and alpha channels. Straight-alpha interpolation
  associates color before filtering and restores the declared mode afterward. Auxiliary channels
  are not scaled. Opaque transitions write one into recognized alpha channels.
- Color tags are semantic declarations, not execution. ICC bytes and named source spaces remain
  separate from the authoritative `ColorSpace`. Ordinary operations preserve tags. No image
  operation evaluates ICC, applies a transfer function, converts a gamut, adapts a white point,
  tone maps, or renders scene values to a display.
- `ImageLimits` is finite by default, and checked producers validate dimensions, counts, arithmetic,
  and allocation before constructing results. It is mainly an operation and decode policy, not a
  global process-memory cap: retained source bytes and decoded planes can coexist, and
  `ImageStorage::new` itself does not apply a caller limit.
- Sequence manifests are immutable discovery snapshots. Logical image number is distinct from
  signed file-frame number and from an optional actual held source frame. Exact, held, and black
  reads retain the requested project media identity and logical number.
- Sequence output is no-clobber. Existing paths, including symlinks, cause conflict. The counter
  advances only after hard-link publication. This protects against replacement but is not a writer
  lock or a guarantee of directory-entry durability after abrupt power loss.
- Writers reject unrepresentable properties rather than silently normalizing them. `write_path`
  currently creates or truncates the destination before all representation validation, so rejected
  output can leave an empty or partial file.
- Structured errors originate in `superi-core` and add component, operation, and field context.
  Caller contract failures are generally `InvalidInput`; unsupported representation is
  `Unsupported`; size and address failures are `ResourceExhausted`; malformed external bytes are
  `CorruptData`; file failures map to `NotFound`, `PermissionDenied`, or retryable `Unavailable`;
  collisions use `Conflict`; impossible validated-state contradictions use terminal `Internal`.
  `ReferenceComparison::require_match` reports an `Internal` degraded validation failure.
- Public immutable descriptors, storage, images, access values, previews, and reference requests are
  designed for background sharing where their contracts assert `Send + Sync`. Mutable sequence
  publication state stays inside the writer.

## Tests and verification

The twelve integration contract files exercise all public modules with real dense values, host
storage, still-image buffers, temporary filesystem sequences, CPU operations, and malformed input.
There are no production inline unit-test modules and no checked-in binary golden fixtures.

Dense fixtures cover U8, U16, F16, and F32, including negative and above-one HDR values,
subnormals, infinities, signed zero, and noncanonical NaN payloads. Geometry fixtures use negative
and nonzero origins plus distinct data and display windows. Storage fixtures cover interleaved and
planar layouts, mixed precision, nonzero plane origins, padded strides, relative alignments, edge
tiles, multiple backing allocations, and mip levels. Channel fixtures cover nested layers,
component alpha, depth, IDs, custom text, Unicode, case distinctions, and deliberately malformed
dot patterns that remain exact base names.

I/O tests use in-memory cursors and temporary directories. They prove representative PNG, JPEG,
TIFF, WebP, TGA, BMP, EXR, and DPX behavior, including RGBA16 PNG, multipart mixed-precision EXR,
tiled mip EXR, EXR metadata, DPX packing and endianness, concrete sequence write/discover/read,
held identity, semantic-black structure, collision refusal, temporary cleanup, and retry. This
coverage does not prove every compression, metadata, profile, descriptor, malformed input, or
format combination.

`resource_limits_contract.rs` pins selected exact memory thresholds and verifies selected malformed
PNG, DPX, and EXR inputs return structured errors without panic. `cpu_reference_contract.rs` proves
deterministic operation dispatch, descriptor and metadata comparison ordering, exact integer and
float behavior, tolerant IEEE cases, and first-mismatch diagnostics. External proof exists in
`superi-gpu` for one explicit GPU readback against the CPU alpha oracle and in `superi-media-io`
for decoded PCM to waveform-envelope to raster flow.

The shard mapping pass did not execute the test suite, so source and test presence alone are not a
claim that the tests currently pass. The map metadata and inventory are verified by the repository
mapping script against all 26 owned files.

## Current status and risks

The module is substantive and broadly contract-tested. There are no `TODO`, `FIXME`, `todo!`, or
`unimplemented!` markers in the mapped implementation. Incomplete behavior is expressed mainly as
structured `Unsupported` results and explicit representation boundaries.

- Dense values do not support U32 samples, mixed precision, planar YUV, arbitrary strides, tiles,
  deep data, or multipart layers. Dense operations offer nearest and bilinear filtering only and
  are synchronous, eager, single-process CPU kernels.
- Dense output operations generally use `ImageLimits`, but some encode paths allocate with
  `Vec::with_capacity`, write APIs have no limits parameter, and alpha image conversion clones
  metadata without applying `max_metadata_bytes`. Very large caller-built exports can therefore
  reach allocator behavior instead of the same structured resource path as bounded decode.
- EXR excludes deep data, subsampled channels, ripmaps, native U8/U16 output, typed timecode output,
  and reconstructed declared color without chromaticities mapping. Attribute projection covers a
  selected type subset. Retained original EXR state preserves unsupported attributes only for
  unchanged same-format output. Reconstructed EXR sorting can canonicalize file channel declaration
  order while keeping samples name-associated. Alpha inference sees final-component `A`, not
  component alpha names such as `AR`, `AG`, or `AB`.
- Raster reconstruction cannot represent signed or cropped windows, tiling, mips, arbitrary channel
  names, mixed precision, or most typed metadata. Current TIFF, TGA, and BMP reconstruction rejects
  ICC. Retained original bytes conceal those model gaps only for unchanged same-format output.
- DPX excludes multiple image elements, RLE, axis-swapped orientations, negative source offsets on
  write, and descriptors beyond implemented luma and RGB variants. Timecode is retained as raw bits
  rather than promoted to typed `Timecode`.
- `composite_over` rejects unrecognized auxiliary channels, and black substitution recognizes alpha
  by conventional name. Custom semantic channels require an explicit conversion convention.
- Reference comparison supports exact or one absolute tolerance, reports only the first sample
  mismatch, and skips sample comparison after descriptor mismatch. It is a validation oracle, not a
  production fallback or a GPU transfer system.
- Native tiled access requires a fully materialized complete mip chain with homogeneous layout.
  Regions remain borrowed per-sample views rather than contiguous assembled buffers. Mip dimensions
  shrink while retaining the base signed origin, which consumers must not reinterpret as scaled
  origin semantics.
- Waveform output is a complete CPU RGBA8 raster without antialiasing, sparse/vector storage,
  multiresolution representation, or GPU execution. The generic channel limit is also applied to
  source audio channel count even though the image output always has four channels.
- Sequence discovery requires UTF-8 canonical filenames and does not refresh after construction.
  Black-reference choice prefers an earlier frame then any later frame, and heterogeneous files can
  make black representation depend on that reference. Hard-link publication depends on filesystem
  support, does not synchronize the containing directory, and handled-error cleanup cannot remove a
  temporary file after abrupt process termination.
- All codec reconstruction and dense processing paths copy through CPU buffers. There is no
  zero-copy bridge from `ImageStorage` to codec state or GPU memory, and `ByteAlignment` must not be
  treated as proof of backing-pointer alignment for SIMD or upload code.

## Maintenance notes

- Run `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py files superi-image` and
  `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py hash superi-image` after any owned
  source or test change. Update the complete inventory, hash, file count, affected prose, and
  `mapped_at_commit`; never update only the hash.
- Changes to channel identity or alpha lookup affect dense transforms, compositing, EXR alpha
  inference, raster and DPX validation, black synthesis, color consumers, GPU validation, and
  sequence/access tests. Preserve exact full names and explicit semantic recognition.
- Changes to metadata or color tags affect all codec projection and rejection paths, dense operation
  preservation, preview/reference identity, `superi-color`, and downstream cache or media contracts.
  Keep source ICC/named payloads separate from authoritative interpretation.
- Changes to storage addressing or native access affect still import/export, black synthesis,
  regions, tiles, previews, and any downstream upload bridge. Preserve complete-row validation,
  stable channel indices, signed coordinates, and the distinction between relative and allocation
  alignment.
- Changes to limits must be reconciled across decode preflight, dense and alpha allocation, tile and
  sequence construction, preview generation, and encode paths. A producer that skips the shared
  policy can reopen an unbounded allocation boundary.
- New still formats or reconstructed capabilities require matching dispatch, extension aliases,
  native precision and channel mapping, alpha/color/metadata policy, source-retention behavior,
  representability errors, tests, and media routing updates.
- Any dense U32 support, sparse/lazy tiling, new resampling filters, custom-alpha convention, GPU
  execution or readback, or sequence publication mechanism changes this module's public boundary
  and must update maps for affected direct consumers.
- Keep consumer maps explicit that `superi-color` owns color transforms, `superi-gpu` owns device
  resources and transfers, and `superi-media-io` owns PCM interpretation. Do not describe
  `Image`, `ImageAccess`, `StillImage`, and GPU frames as one generic image buffer.
