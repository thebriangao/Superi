---
module_id: tool-superi-bench
source_paths:
  - open/tools/superi-bench
source_hash: 121b64d3d00780c8983d4b4a53b90c7270a2b31e006f967af2558ae56e1f9fe9
source_files: 4
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-bench` owns the stable offline benchmark boundary for decode, graph evaluation, upload,
playback, cache, render, and project save/load. It is repository tooling outside the runtime crate
DAG. It records real registered work with fixture provenance and reports absent consumers as gaps
instead of fabricating performance results.

## Source inventory

- `open/tools/superi-bench/Cargo.toml`: Declares the workspace package, core and graph path
  dependencies, shared lint policy, and stable custom benchmark target with `harness = false`.
- `open/tools/superi-bench/src/lib.rs`: Implements stage identities, bounded configuration,
  validated run context, workload registration, sampling, integer statistics, stable JSON output,
  explicit measured, gap, and failed statuses, and the shipped public graph evaluator workload.
- `open/tools/superi-bench/benches/engine.rs`: Reads bounded environment configuration, registers
  available real workloads, executes the suite, prints or writes one report, and returns distinct
  configuration and workload failure statuses.
- `open/tools/superi-bench/tests/harness_contract.rs`: Proves stage order, validation, warmup
  exclusion, deterministic statistics, filtering, JSON escaping, gap and failure honesty,
  registration rules, and the real lazy graph evaluator consumer.

## Public surface

The library exports `BenchmarkStage`, `BenchmarkConfig`, `BenchmarkContext`, `BenchmarkSuite`,
`BenchmarkReport`, result and status types, and `register_graph_evaluation_workload`. A workload
must provide a unique stage, nonempty name, exact fixture identity, and fallible callable. The
runner accepts `SUPERI_BENCH_WARMUP`, `SUPERI_BENCH_SAMPLES`, `SUPERI_BENCH_STAGES`, the documented
context variables, and optional `SUPERI_BENCH_REPORT` output.

## Architecture and data flow

The runner validates bounded configuration and environmental provenance, registers the available
consumer workloads, and executes selected stages in canonical order. Warmup calls are excluded from
measurement. Each measured call is wrapped by a monotonic clock and `std::hint::black_box`, then
integer min, max, mean, p50, and p95 statistics are emitted in schema-versioned JSON.

The graph workload builds a deterministic three-node, two-edge `DirectedAcyclicGraph` once. Every
invocation pulls its output through `LazyEvaluator`, verifies the exact value and three evaluated
keys, and exposes the result to the optimization barrier. The other six stages remain visible gaps
until their owners provide bounded real consumers.

## Dependencies and consumers

The tool depends directly on `superi-core` for exact time and geometry and on `superi-graph` for DAG
storage and lazy evaluation. Cargo invokes the custom benchmark binary, and contributors or CI may
consume its JSON report. No runtime crate depends on this tool.

## Invariants and operational boundaries

- Every iteration count is nonzero and at most 1,000,000.
- Selected stages retain canonical order and cannot contain duplicates.
- A measured result always names its workload and fixture; an unregistered stage is always a gap.
- Workload errors and a backwards clock are failures, never measurements.
- Incomplete hardware or revision context is marked diagnostic through `context_complete: false`.
- The tool remains offline and does not enter the open runtime dependency DAG.

## Tests and verification

Eight contracts exercise configuration and context rejection, exact warmup and sampling behavior,
statistics, stage filtering, error and gap output, stable JSON escaping, duplicate rejection, and a
real graph pull. Final delivery verification also runs the optimized custom benchmark and widening
workspace gates from a checkpoint-specific Cargo target directory.

## Current status and risks

All seven permanent harness identities and the stable runner are implemented. Graph evaluation has
a real registered workload. Decode and upload have substantive lower-level contracts but no bounded
tool-owned fixture consumer yet; playback, cache, render, and project persistence remain absent.
Timer noise means timing values are observations, while ordering, aggregation, schema, and fixture
identity remain deterministic.

## Maintenance notes

Register a new stage workload only after reading its actual public consumer path and choosing a
versioned reproducible fixture. Keep the runner, report schema, structure documentation, tests, and
this map synchronized. Recompute the source hash and rerun the map validator after every owned
source, test, manifest, or benchmark change.
