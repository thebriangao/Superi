---
module_id: superi-cli
source_paths:
  - open/crates/superi-cli
source_hash: 7ca8f9864e08d558c496188cb9ed3893662e4d0d6087e52c940692f917936655
source_files: 10
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cli` is the workspace's headless public API and durable local project consumer. It owns
strict project, media, timeline, render, inspect, validate, recovery, and JSON-RPC automation
workflows plus the normalized process contract for `superi.slice.canonical.v1`. The local workflows
retain filesystem paths as operating-system values, read bounded JSON from a non-symlink file or
one explicit stdin source, inject a deny-by-default permission context, and emit success only after
the API-owned host durably publishes the selected project snapshot. Automation processes strict
JSON Lines in input order and flushes each caller-ID-correlated response before advancing.

The canonical slice validates the authoritative repository fixture,
executes canonical editorial actions through `superi-api`, proves exact reversal, writes the strict
eight-stage report, verifies the revisioned expectation fixture, records bounded timing and process
resident-memory evidence, and publishes a clearly labeled non-playable contract artifact.
Each mutation uses one optimistic typed transaction and consumes one matching ordered replacement
event before the next stage proceeds.
Its portable project-state observation removes checkout location from the canonical digest without
weakening the exact undo and redo comparison or changing the path reported to clients.
The exact `engine validate` command is a second public API consumer. It asks the API-owned
standalone validation helper for one fresh starting-engine observation and prints a strict
deterministic JSON snapshot containing canonical introspection, scenario reversal, lifecycle and
recovery actions, workflow admission, playback, and export state.
The exact `api schema` command is a third public API consumer. It asks `PublicApiSchemaApi` for the
same deterministic catalog used by typed clients and prints all current command, query, event,
resource, error, capability, and permission schemas, including the complete asynchronous job
control vocabulary, complete editor-state discovery, the bounded local scripting command, bounded
event subscription control and polling, and the stateless version negotiation query, without
reconstructing registry data in the CLI. It also reports the permission-free extension discovery
query, replacement event and resource, lifecycle and capability DTOs, and stable public control
reference without attaching a runtime registry. The canonical scenario consumer binds one
exact read permission for its resolved canonical fixture and grants no repository-wide filesystem
authority.

Durable project execution uses only `superi-api::local` and existing strict public DTOs. The CLI
does not import engine, project, timeline, graph, or concurrency types. Media and timeline commands
are fail-closed action partitions, render submission remains absent because no truthful public mux
and publication owner exists, and render configuration is durable project settings rather than a
mock export.

The current runner satisfies contract conformance only. It does not open or decode media, evaluate
pixels, apply production color, encode AV1, mux WebM, or claim a working editor export. Every absent
production owner is explicit in stage diagnostics and the artifact name.

## Source inventory

- `open/crates/superi-cli/Cargo.toml`: Declares `serde`, `serde_json`, `sha2`, `sysinfo`,
  `superi-core`, and `superi-api`, plus `os-codecs` forwarding to the API.
- `open/crates/superi-cli/src/commands.rs`: Implements top-level dispatch, exact legacy argument parsing, repository and
  fixture resolution, bounded strict manifest validation, canonical API execution, stage and
  digest reporting, instrumentation integration, undo plus redo proof, expectation observation
  wiring, active-feature reporting, checkout-independent project-state normalization,
  revision-fenced transaction and event agreement, collision-safe publication, structured exit
  errors including preserved classification, recoverability, bounded structured contexts, and
  redacted path-shaped fields, exact `api schema` and `engine validate` dispatch with strict JSON
  output, explicit exact fixture-read permission binding, and focused portable-digest and
  dispatcher-consumer contracts.
- `open/crates/superi-cli/src/expectations.rs`: Strictly resolves the derived slice expectation
  fixture, validates both parent identities, reference frames, synchronized PCM samples,
  timestamps, project states, and export metadata, then returns stable contract evidence. Focused
  tests prove canonical success, fixture corruption rejection, and modeled-state mismatch handling.
- `open/crates/superi-cli/src/instrumentation.rs`: Implements one reusable current-process sampler,
  monotonic stage probes, resident-set boundary records, and the report instrumentation summary.
- `open/crates/superi-cli/src/main.rs`: Passes process arguments to the private command owner and
  exits with its exact status.
- `open/crates/superi-cli/src/project_workflows.rs`: Implements repeated-option rejection,
  operating-system path retention, bounded strict JSON and JSONL input, non-symlink policy loading,
  deny-all default authority, the complete durable local command grammar, domain filtering through
  the API host, compact one-value output, and per-request flushed JSON-RPC automation.
- `open/crates/superi-cli/tests/api_schema_cli_contract.rs`: Proves deterministic exact schema
  discovery output, catalog and primitive identity, all seven schema categories, exact current counts
  and method names including the generic project and local script commands, asynchronous job query
  and controls, complete editor-state discovery, event subscription open, close, and poll, and API
  and project version negotiation, plus extension discovery and its permission-free classification,
  the permission vocabulary and per-method metadata, help coverage, and invalid usage status.
- `open/crates/superi-cli/tests/scenario_runner.rs`: Provides process contracts for two-run
  reproducibility, exact state and schema 1.1.0 report contents, all-stage timing and nonzero
  resident-memory evidence, exact expectation evidence, honest stub evidence, collision
  preservation, hosted workflow baseline command coverage, help, version, usage, and status 2
  invalid input.
- `open/crates/superi-cli/tests/integration_validation_cli_contract.rs`: Proves deterministic exact
  `engine validate` output, strict public schema identity, coherent startup action and three workflow
  denials, unattached endpoint state, empty findings, help coverage, and invalid usage status.
- `open/crates/superi-cli/tests/project_workflows_cli_contract.rs`: Proves every top-level workflow
  family through the real process, no-clobber create, generic and domain-filtered dispatch, durable
  render configuration, coherent render and editor inspection, validation, copy, backup, recovery,
  JSON-RPC ID echo and response flushing before later failure, reopen equality, strict parser
  failures, deny-by-default permissions, and path-redacted error context.

## Public surface

This crate produces a binary, not a library. Its normalized scenario invocation is:

```text
superi-cli slice run --scenario superi.slice.canonical.v1 \
  --artifact-dir <EMPTY_DIRECTORY> --report <REPORT_JSON>
```

Its normalized engine integration validation invocation is:

```text
superi-cli engine validate
```

Its public schema discovery invocation is:

```text
superi-cli api schema
```

Its durable local project surface is:

```text
superi-cli project create --project <PROJECT> --request <JSON_OR_->
superi-cli project execute --project <PROJECT> --request <JSON_OR_-> [--permissions <JSON>]
superi-cli project inspect --project <PROJECT>
superi-cli project save-copy --project <PROJECT> --destination <PROJECT> --collision <require-absent|replace-existing>
superi-cli project backup --project <PROJECT> --destination <PROJECT>
superi-cli project recovery <get|compare|restore|dismiss> --project <PROJECT> --recovery-root <DIRECTORY> --request <JSON_OR_-> [--permissions <JSON>]
superi-cli media execute --project <PROJECT> --request <JSON_OR_-> [--permissions <JSON>]
superi-cli timeline execute --project <PROJECT> --request <JSON_OR_-> [--permissions <JSON>]
superi-cli render inspect --project <PROJECT>
superi-cli render configure --project <PROJECT> --request <JSON_OR_->
superi-cli inspect <editor|api-schema> [--project <PROJECT>]
superi-cli validate <project|engine> [--project <PROJECT>]
superi-cli automation run --project <PROJECT> --input <JSONL_OR_-> [--permissions <JSON>]
```

Nonstreaming success prints one compact JSON value. Automation accepts one strict JSON-RPC `2.0`
request per nonblank line, requires a string or integral numeric ID and strict method params, echoes
the ID exactly, flushes one compact response after each durable success, and stops at the first
failure without rolling back earlier acknowledged requests. Permission policy is one strict JSON
file containing a canonical principal and typed rules; omitting it denies all protected operations.
The policy file cannot use stdin, preventing two consumers from racing the same byte stream.

Schema discovery success prints exactly one strict `PublicApiSchemaSnapshot` JSON value containing
catalog schema `1.4.0`, stable primitive revision 1, JSON-RPC `2.0`, 16 commands, 13 queries,
nine events, 12 resources, one error schema, one capability schema, and one permission schema in
canonical order. Every method carries its permission mode and possible kinds. Discovery starts no
engine, routes no operation, and owns no transport, permission authority, or registry state.

Validation success prints exactly one strict `GetEngineIntegrationValidationResult` JSON value to
stdout. Incoherent validation or query failure is one structured `engine.validate` stage error and
does not print a success snapshot.

The artifact directory may be absent or empty, must not be a symlink, and receives
`canonical.webm.contract-stub`. The report path must not exist. Both files use create-only temporary
publication followed by a hard link, so an existing destination is never replaced.

No arguments and `--help` print usage and succeed. `--version` prints `superi 0.0.0`. Invalid input
returns 2, unavailable required capability returns 3, and stage or verification failure returns 4.
Errors are one strict stderr JSON object with category, recoverability, message, and stage ID when a
stage owns the failure. API failures also retain a bounded structured context list with path, target,
payload, secret, and token-shaped values redacted. Slice success prints one stdout JSON summary after
both artifact and report exist.

## Architecture and data flow

Durable project commands remain above the project and engine tiers:

```text
strict options plus bounded JSON, JSONL, and permission policy
  -> superi-api LocalProjectHost
  -> complete ProjectDatabase load
  -> scoped EngineControl and existing typed API facade
  -> canonical result plus correlated replacement event
  -> durable replace, copy, backup, or recovery publication
  -> compact stdout response
```

The CLI never serializes a lower-domain authored type of its own. `project create` uses the API's
strict explicit-ID request and no-clobber publication. `media execute` accepts only media actions;
`timeline execute` accepts root, timeline, graph, and clip-mix actions. Render inspection returns
complete editor state and authoritative settings from one loaded revision, while render configure
accepts only existing render setting keys. Project validation performs a read-only complete
current-schema reconstruction. Recovery attaches the same loaded database and history owner used by
the established API facade. Copy and backup use existing collision semantics.

Automation is a sequence of independent durable requests, not one implicit batch transaction. The
CLI parses and executes one line, serializes and flushes its exact JSON-RPC response, then advances.
A later malformed, stale, denied, or failed line terminates the process with structured stderr and
reports how many earlier requests are already durable. A single compound project command remains
one atomic authored and durable unit through the existing API DTO.

The runner walks working-directory ancestors to locate the Superi repository. It records Git commit
and dirty state plus Rust toolchain, build target, features, and profile. It then reads the strict
schema 1 manifest for `slice/video-cfr` version 1 with a one MiB bound, rejects symlinks and unknown
fields, validates required provenance, verifies the exact regular payload's 64 MiB bound, byte
count, and SHA-256, and only then creates the artifact directory. During final verification it reads
the strict `slice/expectations` version 2 fixture with separate bounds for manifests, JSON, RGBA,
and WAVE payloads. It verifies source and audio parent-manifest hashes before consuming expectations.

Execution uses `ScenarioApi` exclusively. After strict fixture resolution, the CLI creates one
nonserializable host permission context with an exact read grant for that resolved canonical input
path and binds it to the facade before import:

```text
fixture.resolve
  -> exact ApiFilesystemScope read grant
  -> media.import
  -> timeline.edit
  -> timeline.compile
  -> graph.evaluate
  -> color.deliver
  -> media.export
  -> slice.verify
```

The independent validation process path uses the public validation facade:

```text
IntegrationValidationApi from_fresh_engine
  -> engine-owned temporary EngineControl dispatcher and default registry
  -> GetEngineIntegrationValidationResult
  -> strict JSON stdout
```

The independent catalog process path uses the public schema facade:

```text
PublicApiSchemaApi
  -> GetPublicApiSchema
  -> validated PublicApiSchemaSnapshot
  -> strict JSON stdout
```

The CLI neither duplicates the explicit API registration list nor imports engine types for schema
discovery or local project work. It discovers the public permission, local scripting, asynchronous
job, event stream, extension registry, and version negotiation schemas
but exposes no job or subscription query, control, submission, polling, waiting, or result command
of its own, adds no mutable extension registry or privileged plugin route, and adds no separate
negotiation executor. Its local permission policy parser
deserializes the API-owned typed rule vocabulary and creates only a process-owned nonserializable
authority context.

The CLI imports only `superi-api` for this path. The engine owns its legal execution domain and
canonical temporary dispatcher behind that public facade, so the CLI does not project engine state,
poll playback or export workers, or create another lifecycle, recovery, or endpoint owner.
It cannot reach the engine's attached `ProjectCommandHistory` directly. Durable project commands
consume only stable API-owned requests, results, state, events, and the local host composition seam.

The API receives exact import, placement, trim, and mirror actions. Each helper call snapshots the
current revision, creates one caller-identified single-action transaction, dispatches it, drains
exactly one event, and requires matching transaction identity, originating command sequence,
project revision, and complete state. Timeline compilation, pixel
evaluation, color delivery, and media export remain contract stubs. The runner undoes effect and
trim, redoes both, removes only the monotonic revision from comparison, and requires exact final
semantic state recovery without reimport. It then compares the real state digests, 48 modeled
timestamps, and exact target metadata with the expectation record. It independently validates 48
RGBA8 reference-frame hashes and all three WAVEFORMATEXTENSIBLE payloads, including clocks,
channel masks, ordered channel labels, probes, silence boundaries, routing signatures, and the
adjacent-sample continuity bound.

Undo and redo compare complete state with only the monotonic revision removed, so a changed media
path still fails reversal proof. The separate expectation observation replaces the one canonical
absolute fixture path with `open/test-fixtures/slice/video-cfr/v1/input.webm` before hashing. This
makes expectation identity portable across clones and worktrees while the report retains the
observed absolute path.

One `ProcessMemorySampler` resolves the CLI process ID once and refreshes only that process with
memory enabled and task enumeration disabled. Each stage takes one resident-set sample immediately
before its work and one immediately after, for 16 bounded refreshes in a complete run. The same
probe measures monotonic elapsed microseconds. An unavailable or zero resident-memory sample is an
explicit stage failure, not a fabricated value or omitted field.

The contract artifact is deterministic JSON with `playable: false`, six missing runtime owners,
and the planned WebM, AV1, 96 by 54, 24 fps, 48-frame target. It is not named `canonical.webm`.
Report schema 1.1.0 retains repository and fixture identities, state digests, full public state,
eight stage records, backend expectations, target metadata, artifact identity, 48 modeled
timestamps, versioned expectation identity, applicable expectation results, and all stub
diagnostics. Default builds report `default`, while `os-codecs` builds report both `default` and
`os-codecs` without claiming an unused backend ran. Rendered
pixel comparison remains `not_evaluated` because the graph, color, and export stages are stubs.
Rendered audio is `not_applicable` because the fixed slice and its target contain zero audio
streams. Every stage retains its existing `duration_us` and adds resident bytes before and after.
The report summary declares the clock, units, memory metric, boundary sampling, stage count, and
maximum resident value observed across those boundaries. Contract success never becomes runtime
success.

## Dependencies and consumers

- `superi-api` supplies schema discovery, revisioned permission-bound scenario transactions,
  coherent integration validation, every strict project, settings, editor, recovery, and
  automation DTO, and the API-owned local project host. The binary consumes no lower authored or
  persistence type.
- Engine project-history, project database, timeline, graph, audio, recovery, and concurrency types
  are deliberately not dependencies. The CLI continues to reach them only through `superi-api`.
- `serde` and `serde_json` parse strict manifests and serialize state, stages, reports, artifacts,
  summaries, and failures.
- `sha2` computes manifest, payload, semantic state, timeline, graph, operation log, and artifact
  identities.
- `sysinfo` 0.36.1 uses only its `system` feature to refresh resident memory for the current
  process. Default component, disk, network, and user collectors are disabled.
- `superi-core` supplies the classified error and recoverability vocabulary retained by local
  workflow failures; it supplies no authored state or persistence owner.
- `open/ci/run-network-isolated.sh` invokes the exact canonical command with temporary output paths
  after workspace tests and fixture validation inside the isolated namespace.
- `.github/workflows/ci.yml` invokes locked fixture validation and the same normalized command as
  first-class steps in both declared Rust build jobs.
- Root and open-tree READMEs document the command and contract-only result.

No runtime crate consumes this binary. The process contracts, contributor workflow, and isolated CI
harness are its current consumers.

## Invariants and operational boundaries

- The only accepted scenario ID is `superi.slice.canonical.v1` at revision 1.
- Repository fixture bytes are input. The runner never downloads, modifies, regenerates, or accepts
  an arbitrary source path.
- Source and manifest reads are bounded. Fixture identity, inventory, path type, size, and digest
  must pass before editorial state or output is created.
- Expected records and payloads are repository-owned, bounded, strict, non-symlink inputs. Unknown
  fields, parent drift, per-frame drift, PCM metadata or sample drift, timestamp drift, state drift,
  and export drift all fail `slice.verify`.
- Pixel tolerance is normalized absolute 0.001 and PCM16 tolerance is exact zero. These values are
  evidence metadata until a real rendered pixel or audio output exists to compare.
- Expectation version 1 remains immutable historical data. Current version 2 normalizes only the
  canonical source location before project-state hashing; every other state, frame, audio, timing,
  and export expectation remains strict.
- Output paths are create-only and collision safe. Existing content and symlinks are preserved and
  rejected.
- Export is outside engine mutation history. The four mutation records remain import, insert, trim,
  and effect.
- Every CLI mutation is revision fenced and must produce exactly one matching complete-state event
  before stage execution continues. A mismatch is a terminal internal stage failure.
- The canonical import facade receives only one exact read grant for the already resolved fixture.
  Permission denial retains its public category, and the CLI never grants a home, checkout, fixture
  directory, or recursive repository scope.
- Contract stubs are never called runtime, and the non-playable artifact is never called WebM
  output.
- Stage order, implementation identity, input and output summaries, diagnostics, state, and artifact
  bytes are deterministic. Durations, resident-memory samples, the observed boundary maximum, and
  chosen output paths are run-specific evidence.
- Instrumentation performs exactly two current-process memory refreshes per stage. It does not scan
  unrelated processes, spawn a sampling thread, retain an unbounded trace, or claim an intra-stage
  memory peak.
- The runner initiates no network operation and executes with default features in the isolated CI
  path.
- `engine validate` accepts no options, initializes no subsystem action, polls no endpoint, and
  changes no scenario, lifecycle, recovery, playback, or export state. It succeeds only when the
  strict public snapshot reports coherent state.
- `api schema` accepts no options, is deterministic across processes, consumes only the API-owned
  catalog, and changes no engine or project state. Discovering permission metadata or job methods
  does not create host authority, attach, poll, or control an engine queue. Discovering extension
  lifecycle, capability, failure, and control metadata likewise does not attach a registry, grant a
  capability, mutate a project, or expose a runtime handle.
- Workflow options are unordered pairs with exactly one value, duplicate and unknown names are
  rejected, and filesystem paths remain `OsString` or `PathBuf` instead of lossy UTF-8 text.
- Request and automation input is bounded to eight MiB, each automation line is bounded to one MiB,
  named inputs and permission policies must be non-symlink regular files, and only the request may
  explicitly use stdin.
- Local authority denies protected operations by default. A policy file supplies only typed API
  rules and a canonical principal; the CLI never infers a grant from a project or media path.
- Every mutating success follows canonical API dispatch, correlated event drain, and durable project
  publication. No response is written before publication, and JSON-RPC automation flushes each
  successful line before executing its successor.
- Project creation and backup are no-clobber. Save-copy replacement requires the explicit collision
  option. Validation and inspection do not rewrite the project. No-op commands invent neither a
  project revision nor a save.
- Media and timeline entry points reject generic, undo, redo, extension, or cross-domain actions
  outside their declared partitions. Render configure rejects non-render settings, and no command
  claims render submission, container muxing, or artifact publication.
- API failures retain stable category and recoverability. Diagnostic contexts are bounded, and
  fields whose names contain path, target, payload, secret, or token are redacted before stderr.

## Tests and verification

The process contract runs the complete command twice with separate output locations. It proves the
strict report schema and scenario identity, authoritative fixture details, exact eight-stage order,
stub and runtime classifications, canonical timeline, mirror matrix, four-operation log, undo plus
redo recovery, versioned expectation identity, 48 reference frames, explicit tolerances, three
audio cases, eight expectation classifications, non-playable artifact, target stream shape, 48
modeled timestamps, identical stub bytes, schema 1.1.0 instrumentation metadata, all-stage duration
values, two nonzero resident samples per stage, and an exact summary maximum. It requires report
equality after removing only durations, resident values, the observed boundary maximum, and output
paths.

Focused unit contracts prove all applicable canonical observations pass, one changed RGBA payload
is rejected as corruption before comparison, and one changed project-state digest is classified as
a terminal contract mismatch rather than fixture corruption. A command unit contract proves two
checkout roots produce the same portable project digest. A second command unit contract proves the
helper uses a revision-fenced public transaction and consumes its matching event. The process suite
also requires one exact fixture validator and one exact slice command for every declared hosted
Rust build job. It requires
the locked `os-codecs` build and test in the capability-gated platform matrix job, while the Ubuntu
22.04 job remains default-only, and verifies active feature identity. The tests do not claim runtime
media decoding, pixel evaluation, audio rendering, or playable export.

Negative process contracts prove unknown scenario rejection, preservation of a nonempty artifact
directory, preservation of an existing report, exact status 2, and help, version, and usage output.
The focused test does not prove Linux namespace isolation, production media behavior, real output
decoding, or expected pixel comparison. Those remain widening or future-owner evidence.

Two engine validation process contracts prove deterministic semantics across separate invocations,
schema and result identity, nested canonical capability and health state, the exact starting
shared-state initialization action, three explicit workflow denials, endpoint attachment state,
empty coherence findings, help coverage, and precise invalid usage. They do not prove a running
application session, UI rendering, endpoint polling, or long-session recovery.

Two API schema process contracts prove deterministic semantics across separate invocations, exact
catalog, primitive, and JSON-RPC identity, all seven schema sections, exact current counts and method
names including the generic editor, `superi.project.script.run`, and
`superi.api.version.negotiate` registrations, every asynchronous job query and control,
`superi.editor.state.get`, `superi.extensions.get`, `superi.extensions.changed`, the extension
replacement resource, event subscription open, close, and poll, the complete permission vocabulary,
and exact method permission metadata, plus help coverage and precise invalid usage.
They do not prove method routing, wire transport, event delivery, host policy persistence, job
execution, scripting, or broad CLI parity.

Two durable workflow process contracts exercise every new top-level family. They prove real
no-clobber creation, complete inspect, generic project query, timeline no-op, media partition
rejection, durable render configuration and coherent inspection, editor inspection, JSON-RPC ID
echo, exact digest-bound `superi-json` routing, in-order response flushing before a later stale
failure, project validation, copy, backup, recovery discovery, schema and validation aliases,
duplicate and unknown option rejection, permission-stdin rejection, deny-by-default media access,
path-redacted failure context, and exact revision equality after reopening every acknowledged file.
The API host suite separately supplies a real linked-media mutation and proves source, record,
timing, identity, grouping, linking, targeting, and synchronization preservation through durable
reopen.

## Current status and risks

The CLI is now a substantive durable project and automation host, API consumer, canonical contract
runner, deterministic engine integration validation client, and exact public schema discovery
client. Its strongest slice
limitation is intentional: six stages model typed boundaries without production execution. The
fixture payload is digest-validated but its decoded traits are reported as expected contract values
because the current media stage does not open it.

Project, media, timeline, render settings, inspect, validate, recovery, copy, backup, and narrow
JSON-RPC automation workflows are real and reopen-tested. Session undo and redo history remains
process-local by engine design, so separate one-shot invocations durably retain the selected
snapshot but do not claim a persisted command journal. Render submission remains absent because the
current public boundary has no prepared executor, container muxer, or publication owner.

Schema discovery now exposes the stable asynchronous job catalog, but the CLI intentionally has no
job-control parser or runtime attachment. A future headless job workflow must consume the same
API-owned query, command, replacement event, and resource contracts rather than importing the
engine queue or adding a second scheduler.

Schema discovery now also exposes the bounded `superi-json` runtime with payload-derived permission
metadata. The CLI intentionally has no dedicated script path argument or source loader; its
automation workflow accepts the exact digest-bound source through the existing API method and
process permission context. Future adapters must not interpret another language or bypass
`ProjectEditorApi`.

Schema discovery also exposes bounded event registration and polling, but the CLI intentionally has
no live stream owner or subscriber command. A later headless client can consume the same API-owned
stream identity, cursor, gap, and resynchronization contracts without rebuilding event ordering.

Schema discovery exposes the extension registry's exact lifecycle, capabilities, safe failure, and
stable project control reference, but the CLI intentionally has no runtime registry owner or
privileged extension command. Durable extension changes continue through the generic project command
and explicit permission policy.

Schema discovery exposes API and project version negotiation, but the CLI does not duplicate its
selection algorithm or add a separate command. A future transport client must call the registered
typed query through the same public method surface.

The canonical scenario path exercises the same fail-closed permission boundary as other typed
clients while retaining exact behavior under its narrow fixture grant. Local workflows can load one
explicit strict policy file, but the CLI does not invent user identity, persist authority, provide
an operating-system sandbox, or infer plugin or filesystem grants from request paths.

`engine validate` currently constructs a fresh starting engine, so it proves the shared public
query and strict state projection rather than attaching to an already running application process.
The same facade is ready for a UI or test host that owns a live canonical dispatcher.

Boundary samples do not continuously observe allocations inside a stage and are not a peak-memory,
constrained-device, or long-session soak result. They provide a portable stage-local signal for the
continuously working slice while those wider performance suites remain separate owners.

The independent expected fixture now makes source-derived frame identities, tolerances, audio
semantics, timestamps, state, and delivery intent reviewable and reproducible. It cannot compare
runtime pixels because no current stage produces them, so the report must preserve the
`not_evaluated` distinction until production graph, color, and export owners integrate. The runner
uses local `git` and `rustc` commands for reproducibility identity and uses hard links for atomic
create-only publication, which assumes a normal contributor filesystem with hard-link support
inside each destination directory.

## Maintenance notes

Keep argument order, scenario identity, exit statuses, artifact name, report fields, stage IDs, and
stub disclosure synchronized with `docs/vertical-slice.md`, process contracts, isolated CI, and
public guidance. Keep `api schema` delegated to `PublicApiSchemaApi`; never reconstruct method,
event, resource, error, capability, or permission declarations in CLI code. Preserve the exact
resolved fixture read grant when the scenario path changes and never broaden it for convenience.
Keep extension discovery as catalog consumption only. A future extension CLI must reuse the public
query and the existing permission-checked generic project command rather than importing an engine
supervisor, attaching a mutable registry, or inventing another lifecycle or capability model.
Keep durable workflows dependent only on `superi-api`, preserve strict option and byte bounds,
reject symlink inputs, keep authority explicit and deny-by-default, and never print mutation success
before durable publication. Keep media, timeline, and render partitions fail closed. Keep JSON-RPC
version and IDs exact, flush each success before advancing, preserve earlier durable work when a
later line fails, and redact path-shaped diagnostic context. Add a new command family only through
an existing typed public owner and real reopen proof.
Keep both hosted build jobs
synchronized with the locked fixture and normalized slice commands. Keep every mutation behind the
typed transaction helper and preserve exact result and event agreement. Keep stage probes around
each stage when its stub is replaced so the fixed
instrumentation contract is inherited by the production owner. When a production owner replaces a
stub, route through that real subsystem, add consumer proof, update implementation identity and
diagnostics, and raise conformance only after all runtime gates pass. Never rename a contract stub
to `canonical.webm` merely to satisfy a filename.
