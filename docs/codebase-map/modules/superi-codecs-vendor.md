---
module_id: superi-codecs-vendor
source_paths:
  - open/crates/superi-codecs-vendor
source_hash: 9af09be984a6eac16bda37f282f7adbe24e7c252428cd5b7813b25d62924c346
source_files: 8
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-codecs-vendor` is the opt-in MIT host adapter for separately installed ARRIRAW, R3D, and
BRAW worker executables. It owns the revisioned process protocol, worker lifecycle, capability
handshake, conversion between wire values and the safe `superi-media-io` interface, source and
decoder handle lifetimes, and atomic registration into a media backend registry.

The crate is a T1b codec adapter. It contains no vendor SDK, vendor codec implementation, dynamic
library loader, or unsafe code. It does not discover, download, bundle, or silently select workers.
A caller explicitly provides every executable and argument list through `VendorPluginConfig`, and
`superi-engine` includes the adapter only through its `vendor-codecs` feature.

The current protocol is decode-only and CPU-only. It can probe and open a local path or complete
in-memory source, read packets, seek, create a decoder, send and receive, flush, reset, close
handles, and transport typed failures. Shared memory, GPU handles, worker sandbox policy,
signature scanning, permissions UI, heartbeat supervision, quarantine, and vendor RAW encoding
are outside this implementation.

## Source inventory

- `open/crates/superi-codecs-vendor/Cargo.toml`: crate manifest. It declares `serde`, `serde_json`,
  `superi-core`, `superi-image`, and `superi-media-io`.
- `open/crates/superi-codecs-vendor/src/backend.rs`: worker handshake and validation, atomic backend
  registration, media backend implementation, source leases, source RPCs, decoder RPCs, decode-only
  enforcement, and best-effort handle close.
- `open/crates/superi-codecs-vendor/src/convert.rs`: checked translation between protocol wire
  values and `superi-core` or `superi-media-io` values, capability construction, reserved source
  handle metadata, and lowercase hexadecimal encoding and decoding.
- `open/crates/superi-codecs-vendor/src/lib.rs`: crate entry point, public re-exports, stable vendor
  RAW identities, and the complete three-format constant.
- `open/crates/superi-codecs-vendor/src/process.rs`: explicit worker configuration, executable
  resolution and process launch, reader and writer threads, bounded newline framing, request
  correlation, cancellation and deadline handling, termination, and worker-error conversion.
- `open/crates/superi-codecs-vendor/src/protocol.rs`: public protocol revision 1 schema for
  envelopes, requests, responses, manifests, sources, streams, packets, seeks, frames, planes,
  metadata, corruption reports, and classified errors.
- `open/crates/superi-codecs-vendor/tests/fixtures/mock_vendor_worker.rs`: standalone worker fixture
  implementing the successful flow plus hang, invalid JSON, and unterminated-response modes.
- `open/crates/superi-codecs-vendor/tests/vendor_plugin_contract.rs`: crate-level protocol,
  registration, probe, source, relink, decode, lifecycle, timeout, malformed-response, size-bound,
  atomicity, and cancellation contracts.

## Public surface

The crate root exposes:

- `register_vendor_plugins`, which starts, handshakes, validates, and registers an explicit slice
  of workers into a mutable `BackendRegistry`.
- `VendorPluginConfig`, with `new`, `with_arguments`, `with_startup_timeout`, and
  `with_maximum_message_bytes`. Its fields remain private and its defaults are a five-second
  startup limit and a 64 MiB request or response line limit.
- `VendorRawFormat`, with `Arriraw`, `R3d`, and `Braw`, stable lowercase codes, and code lookup.
- `VENDOR_RAW_FORMATS`, the stable complete three-format array.
- the public `protocol` module.

`protocol` exposes `PROTOCOL_REVISION`, currently 1, and all wire schema types:
`Envelope`, `PluginManifest`, `ProtocolRequest`, `ProtocolResponse`, `SourceLocationWire`,
`ProbeResultWire`, `SourceWire`, `StreamWire`, `StreamKindWire`, `TimebaseWire`, `TimeWire`,
`DurationWire`, `SeekWire`, `SeekModeWire`, `PacketWire`, `PacketTimingWire`, `ReadPacketWire`,
`CorruptionWire`, `FrameWire`, `PlaneWire`, `ColorSpaceWire`, `DecoderOutputWire`, `MetadataWire`,
`MetadataValueWire`, and `ErrorWire`.

Requests cover handshake, probe, open, packet read, seek, source close, decoder creation, packet
submission, decoder receive, flush, reset, and decoder close. Responses cover handshake, probe,
open, packet read, seek, decoder creation, decoder output, acknowledgement, and classified failure.
Every structured protocol type uses Serde's unknown-field denial, and tagged enums use stable
snake-case operation names. Metadata is a `BTreeMap`, so serialized key order is deterministic.

The backend implementation types, `ProcessClient`, conversion helpers, source and decoder wrappers,
and reserved metadata key are private. Consumers operate through `superi-media-io` trait objects
rather than vendor-specific source or decoder types.

## Architecture and data flow

Registration begins in `register_vendor_plugins`. For each explicit config, `ProcessClient::start`
resolves the executable, starts it, and creates one writer thread and one reader thread. The host
sends a revision 1 handshake under a startup context whose deadline is the smaller of the caller's
remaining time and the configured startup timeout. The manifest must repeat revision 1, provide
nonempty plugin and SDK versions, declare a nonempty unique set of known vendor formats, and build
a valid media backend descriptor.

Each connected worker becomes one primary backend with priority 500. Its capabilities are exactly
`Source` plus decode operations and unreported codec-detail rows for the manifest's declared
formats. Hardware acceleration remains `Unreported`, because revision 1 does not negotiate a
truthful execution mode. The function connects and validates every worker, preflights backend IDs
against both the existing registry and the new batch, and only then mutates the registry. Dropping
an unregistered process client terminates its child, so a failure leaves registry state unchanged.

Worker startup uses `std::process::Command` with an empty environment, caller-provided arguments,
piped standard input and output, discarded standard error, and the executable's parent directory
as current directory. No inherited environment variable participates in worker behavior.

The process protocol is a single newline-terminated JSON object in each direction. A monotonic host
ID wraps every request, and the response must repeat it. Before the newline is appended, serialized
requests must fit the configured limit. The reader consumes at most that many payload bytes before
one newline and rejects EOF with an unterminated partial line. A single process mutex permits only
one in-flight request, while capacity-one writer and reader channels provide bounded thread
handoff.

`OperationContext` remains active while a caller waits for the process mutex, writer completion,
or reader output. These waits poll in ten-millisecond slices. A timeout, cancellation after a write
has begun, channel failure, I/O error, oversized response, invalid JSON, unterminated line, closed
stream, or mismatched response ID terminates and waits for the worker. A valid worker `Failure`
response is converted to a host `Error` after its category, recoverability, and nonempty message
are validated; it does not automatically terminate the worker.

Probe converts the bounded source prefix to lowercase hexadecimal and sends source name,
extension, total length, and completeness. `NoMatch` passes through. `NeedMoreData` is rebuilt with
a nonzero host constructor. `Match` is accepted only for a format declared by the manifest, and
confidence is rebuilt through the host's 1 through 100 bound.

Open sends the canonical media ID, a UTF-8 path or complete in-memory bytes encoded as lowercase
hexadecimal, and an optional expected fingerprint. The response must provide a nonempty opaque
source handle and fingerprint. Relink fingerprints must match exactly. Every stream is rebuilt
with checked IDs, kinds, codecs, timebases, durations, and metadata. Vendor video codecs must be
declared by the worker, other video codecs are rejected, and at least one declared vendor RAW video
stream must exist. Non-video streams may use other codec IDs.

For each vendor RAW stream, the host rejects worker-supplied use of the reserved
`superi.vendor.source_handle` metadata key, then inserts the validated source handle itself. This
metadata links a later `DecoderConfig` to a live `SourceLease`. A lease owns the process and handle;
the source and all decoders clone it. The worker receives `CloseSource` only when the last lease is
dropped, so a decoder can outlive the `MediaSource` that created its stream description.

Packet reads rebuild complete or partial `ReadOutcome` values. Packet bytes, timing, keyframe state,
metadata, corruption kind, recoverability, optional stream ID, and all-or-none byte progress pass
through checked host constructors. Seek supports exact, previous-keyframe, and nearest-keyframe
modes and rebuilds the selected rational time.

Decoder creation rechecks the requested vendor codec, declared capability, reserved source handle,
and live source lease before sending the stream. The returned decoder handle must be nonempty. The
decoder then maps `send_packet`, `receive`, `flush`, and `reset` directly to correlated RPCs.
`receive` rebuilds `NeedInput`, `EndOfStream`, or a CPU `VideoFrame`.

Frame conversion resolves pixel, color, and alpha codes through stable host lookups. Every plane's
lowercase hexadecimal bytes, stride, and row count build a `VideoPlane`; `CpuVideoBuffer` validates
plane count and geometry against width, height, and pixel format; `VideoFormat` validates dimensions
and alpha meaning; and `VideoFrame` validates storage and timing agreement. Revision 1 never returns
a GPU or external frame owner.

Source and decoder drops issue best-effort close requests under a fresh 250-millisecond background
operation. Process drop always kills and waits for the child if it has not already terminated.

## Dependencies and consumers

Direct dependencies are:

- `serde` and `serde_json` for the strict public wire schema and newline JSON serialization.
- `superi-core` for color tags, errors, media IDs, pixel and alpha formats, exact time types, and
  recoverability codes.
- `superi-media-io` for backend registration and selection, source and decoder traits, packets,
  streams, metadata, corruption reports, CPU frames, capabilities, priorities, cancellation, and
  deadlines.

`superi-image` is declared in `open/crates/superi-codecs-vendor/Cargo.toml`, but no current crate
source uses it. Decoded worker frames cross through `superi-media-io::decode::CpuVideoBuffer`, not a
`superi-image::Image`.

The sole runtime consumer is `superi-engine`. `open/crates/superi-engine/Cargo.toml` makes the crate
optional behind `vendor-codecs`. `open/crates/superi-engine/src/media.rs` exposes
`media_backend_registry_with_vendor_plugins`, which first builds the ordinary registry and then
registers the caller's explicit workers. The default registry is vendor-free.

`open/crates/superi-engine/tests/vendor_codec_registry_contract.rs` is the downstream integration
proof. It verifies that default capability introspection contains no vendor formats and that an
explicit fixture worker appears as the primary source and decode backend with unreported detailed
capabilities.

`docs/codecs.md` is the governing policy consumer. It requires the default build to remain free of
vendor code and permits proprietary RAW processing only in separately installed workers behind
this host adapter. The crate has no dependency on `superi-codecs-rs`, `superi-codecs-platform`, or
`superi-engine`, preserving the downward media-interface boundary.

## Invariants and operational boundaries

- No worker is discovered or downloaded. Every executable and argument list is caller-selected.
- No vendor SDK or vendor codec code is linked or loaded into the Superi process.
- Registry mutation is all-or-nothing for one registration call. Every handshake and backend ID is
  validated before the first new registration is inserted.
- The only accepted protocol revision is 1. Unknown JSON fields and unknown tagged variants fail
  deserialization.
- At most one request is in flight per worker. Every successful response must use the exact request
  ID and operation-specific response variant.
- Request and response payload lines are bounded by `maximum_message_bytes`, which must be positive.
  Startup timeout must also be positive.
- Cancellation is checked before executable discovery. Cancellation and deadlines remain active
  across lock, write, and read waits.
- Malformed transport or protocol framing is terminal to the process client. Valid typed worker
  failures remain ordinary operation results.
- Worker manifests may declare any nonempty unique subset of ARRIRAW, R3D, and BRAW. Capabilities
  and accepted probe or stream results are limited to that exact set.
- Vendor backends are decode-only. Encoder creation always returns `Unsupported`, and encode
  capability is never advertised.
- A source must contain at least one declared vendor RAW video stream. Vendor codecs cannot be
  mislabeled as audio or another stream kind, and undeclared or unknown video codecs are rejected.
- The expected relink fingerprint is compared before source state is exposed to the caller.
- The reserved source-handle metadata is host-owned. Decoder creation requires both that metadata
  and a currently live lease.
- Packet, time, duration, corruption, frame, plane, metadata, pixel, color, and alpha values are
  reconstructed through safe public constructors before engine exposure.
- Wire binary data must be lowercase hexadecimal with even length. The host never accepts uppercase,
  malformed, or odd-length payloads.
- Source and decoder handles are opaque strings but must be nonempty. Close is best-effort and
  process teardown is definitive.
- The crate contains no `unsafe` block. The operating-system boundary is process creation and IPC
  through safe standard-library APIs.

## Tests and verification

`open/crates/superi-codecs-vendor/tests/vendor_plugin_contract.rs` compiles
`open/crates/superi-codecs-vendor/tests/fixtures/mock_vendor_worker.rs` with `rustc` and exercises a
real child process. The successful path proves strict handshake serialization, all three stable
format identities, registry capabilities, bounded content probe, memory-source open, fingerprint,
source and frame metadata, packet timing, BRAW decoder creation, CPU RGBA16F frame conversion,
decode-only enforcement, exact seek, reset, flush, end of stream, and relink mismatch.

Failure tests prove a hanging handshake is cut off by the configured deadline, invalid JSON and an
unterminated response are terminal corrupt-data errors, a small protocol limit rejects the
handshake, duplicate backend IDs leave registration atomic, and pre-cancelled work fails before
missing-executable discovery.

`open/crates/superi-engine/tests/vendor_codec_registry_contract.rs` supplies the downstream feature
test. It proves the ordinary engine registry has no ARRIRAW, R3D, or BRAW operations, while the
explicit feature constructor reaches engine capability introspection with the fixture backend and
unreported hardware and codec detail.

The fixture does not exercise every public revision 1 wire variant. In particular, there is no
contract case for partial packet corruption, every metadata value kind, every pixel layout, invalid
frame plane geometry, source path mode, a worker-generated typed `Failure`, response ID mismatch,
oversized worker output, duplicate source or decoder handles, or concurrent callers.

## Current status and risks

The explicit decode-only process adapter is implemented and has a real process integration test.
The current boundary is intentionally a first protocol revision, not the complete extension
security or high-throughput media architecture.

- Revision 1 hex-encodes every binary payload. This doubles packet, memory-source, metadata-byte,
  and frame-plane size before JSON overhead, copies data repeatedly, and prevents zero-copy or
  GPU-resident decode.
- The 64 MiB default line bound is the only aggregate response allocation limit owned by this
  crate. It bounds a whole frame message, but it is not a dedicated decoded-pixel or metadata
  budget. Large professional RAW frames may exceed it after hexadecimal expansion.
- Memory-source hex and JSON are allocated before the serialized request-size check. A request that
  is ultimately rejected can therefore consume substantially more memory than the configured
  wire limit.
- Every worker request is serialized behind one mutex, including independent sources and decoders.
  Long decode or I/O calls block all other work for that worker and make throughput dependent on
  one request-response round trip at a time.
- `Command` discards worker standard error. Spawn and protocol errors are typed, but worker-native
  diagnostics written only to standard error are unavailable to the user.
- Clearing the environment and using a separate process provide isolation, not a sandbox. The
  worker still runs with the launching user's operating-system permissions. Signature checks,
  filesystem grants, network policy, scanning, heartbeat, restart, and quarantine are future
  extension responsibilities described by repository architecture.
- UTF-8 is required for path sources. Valid non-UTF-8 local paths cannot be opened through revision
  1. Relative paths are sent unchanged while the worker's current directory is its executable
  directory, so callers must use a path whose meaning remains correct in that process.
- Source handles are indexed in a weak map but uniqueness is not explicitly enforced. A worker that
  reuses one live source handle can replace the map entry used for later decoder creation. Decoder
  handles are also checked only for nonemptiness, not uniqueness.
- Best-effort close uses the same serialized protocol with a 250-millisecond deadline. A stalled
  close after acquiring process state can terminate the shared worker, while a close that cannot
  acquire the lock in time is silently abandoned until process teardown.
- An operation-specific response variant mismatch becomes a terminal-classified protocol error in
  the backend, but an already registered `ProcessClient` is not explicitly terminated by that
  check. Subsequent requests can still reach the worker.
- Worker `Failure` categories and recoverability are syntax-checked but otherwise trusted. A worker
  can classify its own valid-code failure and provide the user-visible message.
- The request ID exhaustion check runs after `fetch_add`. At the practically unreachable
  `u64::MAX` boundary the atomic has already wrapped, so a later call could reuse low identifiers.
- There is no encode path, no shared-memory transport, no negotiated GPU ownership, no worker
  capability detail beyond format identity, and no public introspection of worker plugin or SDK
  version after handshake.
- `superi-image` is an unused manifest dependency.

## Maintenance notes

After any source change under `open/crates/superi-codecs-vendor`, rerun the mapping script's
`files` and `hash` commands, update both metadata and prose, and run the crate contract plus
`open/crates/superi-engine/tests/vendor_codec_registry_contract.rs` with `vendor-codecs`. Any new
source or fixture file must appear in the inventory.

Any protocol change must either preserve revision 1 byte-for-byte behavior or introduce a new
negotiated revision. Update `open/crates/superi-codecs-vendor/src/protocol.rs`, both conversion
directions, process bounds, fixture worker, crate tests, engine integration tests, and the vendor
worker contract in `docs/codecs.md` together.

Keep capability declarations truthful. Do not advertise encode, hardware acceleration, profile,
bit-depth, chroma, GPU, or shared-memory support until the corresponding negotiated wire fields,
ownership rules, conversions, and end-to-end tests exist.

Changes to source or decoder lifetime must preserve the host-owned reserved metadata key and the
rule that decoders keep a source lease alive. Changes to process supervision must preserve caller
cancellation, bounded reads and writes, atomic registry publication, and definitive child cleanup.

Do not add a vendor SDK dependency, implicit worker discovery, automatic download, or proprietary
artifact to this crate. Such a change would violate the codec boundary in `docs/codecs.md` and the
T1b dependency law in `open/docs/STRUCTURE.md`.
