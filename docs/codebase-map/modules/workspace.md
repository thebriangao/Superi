---
module_id: workspace
source_paths:
  - repository files outside open/crates/* and open/tools/*
source_hash: 714a5340d952d8c07cb631ac5d882e461ad0d9019eec546140b06bac017e8a71
source_files: 260
mapped_at_commit: working-tree
---

## Purpose and ownership

The `workspace` module owns repository law, product and architecture documents, CI policy, Cargo
workspace resolution, generated and preserved TypeScript contracts, brand assets, fixtures, and the
repository-owned checkpoint operating system. Runtime crates and Rust tools have separate maps.

Phase Infinity replaces the retired React, Tauri, Vite, and webview presentation with a retained
native wgpu path. Historical checkpoint documents remain tracked evidence for the revisions that
created them, but they do not override current manifests, source, tests, or this map.

## Source inventory

The inventory is intentionally path-complete. Checkpoint records are historical evidence, binary
entries are fixtures or brand assets, and all other entries are current policy, configuration,
documentation, or contract source.

- `.agents/skills/superi-execution/SKILL.md`: Checkpoint execution instructions.
- `.agents/skills/superi-execution/agents/openai.yaml`: Execution skill metadata.
- `.agents/skills/superi-execution/scripts/verify_checkpoint.py`: Deterministic checkpoint verifier,
  including generated binding and both preserved TypeScript contract lanes.
- `.agents/skills/superi-human-acceptance/SKILL.md`: Human visual acceptance instructions.
- `.agents/skills/superi-human-acceptance/agents/openai.yaml`: Human acceptance skill metadata.
- `.agents/skills/superi-icon-system/SKILL.md`: Original icon-system instructions.
- `.agents/skills/superi-icon-system/agents/openai.yaml`: Icon skill metadata.
- `.agents/skills/superi-integration/SKILL.md`: Integration instructions.
- `.agents/skills/superi-integration/agents/openai.yaml`: Integration skill metadata.
- `.agents/skills/superi-mapping/SKILL.md`: Codebase mapping instructions.
- `.agents/skills/superi-mapping/agents/openai.yaml`: Mapping skill metadata.
- `.agents/skills/superi-mapping/scripts/codebase_maps.py`: Module discovery and map validator.
- `.agents/skills/superi-research-planning/SKILL.md`: Research and planning instructions.
- `.agents/skills/superi-research-planning/agents/openai.yaml`: Research planning skill metadata.
- `.agents/skills/superi-testing/SKILL.md`: Testing instructions.
- `.agents/skills/superi-testing/agents/openai.yaml`: Testing skill metadata.
- `.agents/skills/superi-visual-capture/SKILL.md`: Private retained UI capture instructions.
- `.agents/skills/superi-visual-capture/agents/openai.yaml`: Visual capture skill metadata.
- `.agents/skills/superi-visual-design/SKILL.md`: Visual design instructions.
- `.agents/skills/superi-visual-design/agents/openai.yaml`: Visual design skill metadata.
- `.agents/skills/superi-wgpu-construction/SKILL.md`: Native wgpu construction instructions.
- `.agents/skills/superi-wgpu-construction/agents/openai.yaml`: Wgpu construction skill metadata.
- `.codex/config.toml`: Repository Codex configuration.
- `.github/scripts/check-ci-features.py`: Hosted feature-lane contract.
- `.github/scripts/check-dependency-policy.sh`: Dependency workflow contract.
- `.github/scripts/libvpx-windows.def`: Reviewed Windows libvpx exports.
- `.github/scripts/provision-linux-libva.sh`: Pinned Linux media provisioning.
- `.github/scripts/provision-windows-libvpx.sh`: Pinned Windows libvpx provisioning.
- `.github/workflows/ci.yml`: Cross-platform Rust workflow.
- `.github/workflows/dependency-policy.yml`: Dependency policy workflow.
- `.github/workflows/network-isolated.yml`: Offline execution workflow.
- `.github/workflows/typescript-contracts.yml`: Preserved TypeScript contract workflow.
- `.gitignore`: Repository ignore policy.
- `.worktreeinclude`: Codex worktree include policy.
- `LICENSE`: MIT license.
- `README.md`: Current project orientation.
- `ci/api-client-contract/.node-version`: API client Node pin.
- `ci/api-client-contract/README.md`: API client lane contract.
- `ci/api-client-contract/package-lock.json`: API client dependency lock.
- `ci/api-client-contract/package.json`: API client scripts and dependencies.
- `ci/api-client-contract/src/api-contract.ts`: Generated API consumer contract.
- `ci/api-client-contract/tests/contract.test.mjs`: API consumer tests.
- `ci/api-client-contract/tsconfig.json`: API client strict TypeScript configuration.
- `closed/README.md`: Closed-layer boundary.
- `docs/architecture.md`: Current architecture.
- `docs/checkpoints/P1.W05.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W05.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W05.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W05.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W05.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W05.C012.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C008.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C015.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C017.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C018.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C019.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C020.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C021.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C022.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C023.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C024.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C025.md`: Historical checkpoint evidence.
- `docs/checkpoints/P1.W07.C026.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W02.C013.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C008.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C012.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C013.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W04.C014.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W05.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W05.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W06.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W06.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W06.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W06.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W06.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W06.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W07.C016.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W07.C022.md`: Historical checkpoint evidence.
- `docs/checkpoints/P2.W07.C025.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W01.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W01.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W01.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W01.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W01.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W01.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W02.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W02.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W02.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C008.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C012.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C013.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W03.C014.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C015.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W04.C016.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C008.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W05.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C012.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C013.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C014.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C015.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W06.C016.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C001.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C002.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C003.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C004.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C005.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C006.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C007.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C008.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C009.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C010.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C011.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C012.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C013.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C014.md`: Historical checkpoint evidence.
- `docs/checkpoints/P3.W07.C015.md`: Historical checkpoint evidence.
- `docs/codecs.md`: Codec policy.
- `docs/north-star.md`: Product north star.
- `docs/phase-0-build-contracts.md`: Current build contracts.
- `docs/phases.md`: Historical phase sequence.
- `docs/platform-testing.md`: Platform verification policy.
- `docs/unsafe-ffi.md`: Unsafe and FFI audit policy.
- `docs/vertical-slice.md`: Canonical slice definition.
- `open/Cargo.lock`: Locked Rust dependency graph.
- `open/Cargo.toml`: Cargo workspace membership and dependency policy.
- `open/README.md`: Open-tree orientation.
- `open/assets/brand/128x128.png`: Preserved brand asset.
- `open/assets/brand/128x128@2x.png`: Preserved brand asset.
- `open/assets/brand/32x32.png`: Preserved brand asset.
- `open/assets/brand/README.md`: Brand asset provenance.
- `open/assets/brand/app-icon.svg`: Preserved vector brand source.
- `open/assets/brand/icon.icns`: Preserved macOS brand asset.
- `open/assets/brand/icon.ico`: Preserved Windows brand asset.
- `open/assets/brand/icon.png`: Preserved brand asset.
- `open/bindings/typescript/editorial-contracts/.node-version`: Editorial contract Node pin.
- `open/bindings/typescript/editorial-contracts/README.md`: Editorial contract boundary.
- `open/bindings/typescript/editorial-contracts/package-lock.json`: Editorial contract dependency lock.
- `open/bindings/typescript/editorial-contracts/package.json`: Editorial contract scripts and dependencies.
- `open/bindings/typescript/editorial-contracts/src/api.ts`: Generated API facade.
- `open/bindings/typescript/editorial-contracts/src/playback-transport.ts`: Playback transport contract.
- `open/bindings/typescript/editorial-contracts/src/project-lifecycle.ts`: Project lifecycle contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-captions.ts`: Caption contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-clip-presentation.ts`: Clip presentation contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-editing.ts`: Timeline editing contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-editorial-feedback.ts`: Editorial feedback contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-multicam.ts`: Multicam contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-nesting.ts`: Timeline nesting contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-retime.ts`: Retime contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-transition-presentation.ts`: Transition contract.
- `open/bindings/typescript/editorial-contracts/src/timeline-workspace.ts`: Timeline workspace contract.
- `open/bindings/typescript/editorial-contracts/tests/playback-transport.test.ts`: Playback contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-captions.test.ts`: Caption contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-clip-presentation.test.ts`: Clip contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-editing.test.ts`: Editing contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-editorial-feedback.test.ts`: Feedback contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-multicam.test.ts`: Multicam contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-nesting.test.ts`: Nesting contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-retime.test.ts`: Retime contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-transition-presentation.test.ts`: Transition contract tests.
- `open/bindings/typescript/editorial-contracts/tests/timeline-workspace.test.ts`: Workspace contract tests.
- `open/bindings/typescript/editorial-contracts/tsconfig.json`: Strict editorial TypeScript configuration.
- `open/bindings/typescript/superi-api.ts`: Canonical generated TypeScript binding.
- `open/ci/network-isolated-contract.sh`: Offline workflow contract.
- `open/ci/run-network-isolated.sh`: Offline execution wrapper.
- `open/deny.toml`: License and source policy.
- `open/docs/STRUCTURE.md`: Open-tree dependency structure, including retained UI, portable
  session, and thin native host tiers.
- `open/rust-toolchain.toml`: Rust toolchain contract.
- `open/rustfmt.toml`: Rust formatting policy.
- `open/test-fixtures/README.md`: Fixture catalog.
- `open/test-fixtures/audio/synchronized-multichannel/v1/fixture.json`: Multichannel audio fixture metadata.
- `open/test-fixtures/audio/synchronized-multichannel/v1/stereo-44100.wav`: Stereo audio fixture.
- `open/test-fixtures/audio/synchronized-multichannel/v1/surround-5-1-48000.wav`: 5.1 audio fixture.
- `open/test-fixtures/audio/synchronized-multichannel/v1/surround-7-1-96000.wav`: 7.1 audio fixture.
- `open/test-fixtures/color/image-sequences/v1/fixture.json`: Color sequence fixture metadata.
- `open/test-fixtures/color/image-sequences/v1/image-cases.csv`: Color image cases.
- `open/test-fixtures/color/image-sequences/v1/image-samples.bin`: Color image samples.
- `open/test-fixtures/color/image-sequences/v1/sequence-cases.csv`: Color sequence cases.
- `open/test-fixtures/golden/harness/v1/audio.json`: Golden audio expectations.
- `open/test-fixtures/golden/harness/v1/fixture.json`: Golden harness metadata.
- `open/test-fixtures/golden/harness/v1/frame.json`: Golden frame expectations.
- `open/test-fixtures/golden/harness/v1/project.json`: Golden project expectations.
- `open/test-fixtures/golden/harness/v1/timeline.json`: Golden timeline expectations.
- `open/test-fixtures/media/error-cases/v1/fixture.json`: Media error fixture metadata.
- `open/test-fixtures/media/error-cases/v1/malformed.wav`: Malformed audio fixture.
- `open/test-fixtures/media/error-cases/v1/media-error-cases.csv`: Media error cases.
- `open/test-fixtures/media/error-cases/v1/partial-readable.wav`: Partial audio fixture.
- `open/test-fixtures/media/error-cases/v1/truncated.aiff`: Truncated AIFF fixture.
- `open/test-fixtures/media/error-cases/v1/unsupported.aifc`: Unsupported AIFC fixture.
- `open/test-fixtures/policy/utf8/v1/fixture.json`: UTF-8 fixture metadata.
- `open/test-fixtures/policy/utf8/v1/hello.txt`: UTF-8 fixture payload.
- `open/test-fixtures/slice/expectations/v1/expectations.json`: Slice v1 expectations.
- `open/test-fixtures/slice/expectations/v1/expected-frames.rgba`: Slice v1 frame bytes.
- `open/test-fixtures/slice/expectations/v1/fixture.json`: Slice v1 fixture metadata.
- `open/test-fixtures/slice/expectations/v2/expectations.json`: Slice v2 expectations.
- `open/test-fixtures/slice/expectations/v2/expected-frames.rgba`: Slice v2 frame bytes.
- `open/test-fixtures/slice/expectations/v2/fixture.json`: Slice v2 fixture metadata.
- `open/test-fixtures/slice/video-cfr/v1/fixture.json`: CFR video fixture metadata.
- `open/test-fixtures/slice/video-cfr/v1/input.webm`: CFR video fixture.
- `open/test-fixtures/timeline/otio-interchange/v1/canonical-slice.otio`: Canonical OTIO slice.
- `open/test-fixtures/timeline/otio-interchange/v1/expectations.json`: OTIO expectations.
- `open/test-fixtures/timeline/otio-interchange/v1/fixture.json`: OTIO fixture metadata.
- `open/test-fixtures/timeline/otio-interchange/v1/interchange-coverage.otio`: OTIO coverage fixture.
- `open/test-fixtures/timing/cadences/v1/fixture.json`: Cadence fixture metadata.
- `open/test-fixtures/timing/cadences/v1/timing-cases.csv`: Cadence cases.
- `open/test-fixtures/video/pixel-formats/v1/fixture.json`: Pixel-format fixture metadata.
- `open/test-fixtures/video/pixel-formats/v1/video-cases.csv`: Pixel-format cases.
- `open/test-fixtures/video/pixel-formats/v1/video-frames.bin`: Pixel-format bytes.
- `tools/superi-capture`: Root private retained UI capture wrapper.

## Public surface

The workspace surface is its repository contract: Cargo membership and dependencies, policy
documents, CI workflows, generated TypeScript binding, preserved editorial contract packages,
fixtures, and checkpoint skills. Root `AGENTS.md` is local operational law and is intentionally
ignored, so it is reread independently and is not part of this map hash.

## Architecture and data flow

`open/Cargo.toml` resolves the Rust graph. The native presentation path is
`superi-desktop -> superi-ui -> superi-gpu`, with `superi-session` coordinating existing engine and
project owners. The generated TypeScript binding is produced by the Rust binding tool and consumed
by the two isolated contract packages. CI runs Rust, dependency, offline, and TypeScript contract
lanes. The checkpoint verifier selects a deterministic local superset from the complete diff.

Brand assets moved intact from the retired host into `open/assets/brand`. Private visual capture is
exposed through `tools/superi-capture` and uses the retained wgpu compositor. Plans and captures live
under ignored `plans/` and never enter commits.

## Dependencies and consumers

Every crate and tool consumes workspace dependency pins and policy. GitHub Actions consume the
workflow scripts, fixtures, and manifests. Native UI checkpoints consume the repository-owned skill
sequence. The TypeScript lanes consume only the committed generated API and preserved editorial
contracts, with no production JavaScript runtime.

## Invariants and operational boundaries

- The open product remains offline-first and Rust-owned.
- The production presentation has no React, Tauri, Vite, webview, or browser dependency.
- Historical checkpoint evidence is preserved but cannot define current runtime ownership.
- Brand, fixture, media, and audio assets remain byte-preserved unless a checkpoint changes them.
- `closed/` may depend on `open/`; `open/` may not depend on `closed/`.
- Plans, captures, claims, and human-acceptance state are local working artifacts.
- Source-changing checkpoints update every affected map and pass the global validator.
- TypeScript packages remain contract consumers rather than a second application owner.

## Tests and verification

Workspace proof includes the map validator, Cargo formatting, locked build and tests, strict
all-target Clippy, generated binding drift, open-tree boundary enforcement, dependency policy,
fixture validation, canonical slice execution, network-isolated contracts, native smoke, retained
UI captures, and strict TypeScript tests. The full checkpoint verifier records the exact selected
commands.

## Current status and risks

The Phase Infinity foundation is present: native retained UI, portable session services, private
capture, revised skills, preserved contracts, and retired presentation removal. The UI is a
foundation rather than the finished editor. Historical documents still name the retired stack by
design, so current architecture readers must begin with this map and the live source.

## Maintenance notes

Regenerate the exact inventory whenever any non-crate or non-tool repository file changes. Update
architecture, workflow, skill, contract, fixture, and asset descriptions with the same change.
The C001 final refresh rechecked all 260 entries after skill formatting cleanup and found no
inventory, ownership, relationship, or behavior change. Never refresh only the hash or file count.
