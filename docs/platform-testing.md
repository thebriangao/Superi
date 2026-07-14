# Hardware and operating-system test matrix

**Status:** Required development and release evidence
**Matrix revision:** 2
**Platform snapshot:** 2026-07-13

This document defines the operating-system and hardware coverage required to call a Superi change
portable. It does not turn a virtual build into hardware evidence. Every lane must run the real open
tree, use deterministic fixtures where the suite permits them, and record gaps instead of silently
substituting another platform.

The product boundary remains macOS, Windows, and Linux through wgpu's native Metal, D3D12, and
Vulkan backends. The default codec configuration is required everywhere. The optional `os-codecs`
configuration is required only where the host exposes the platform API and driver capability that
the repository contract describes.

## Support policy

Required operating-system coverage follows supported vendor releases rather than an unbounded list
of old versions. A release is removed only after its vendor support ends and one full Superi release
has passed with the replacement lane. Preview operating systems and preview runner images are
informational until promoted here.

The current required client systems are:

| Platform | Required versions | Required architectures | Native GPU backend |
| --- | --- | --- | --- |
| macOS | 26, 15, 14 | arm64; x86_64 on 15 or 26 | Metal |
| Windows 11 | 25H2, 24H2 | x86_64 | D3D12 |
| Ubuntu LTS | 26.04, 24.04, 22.04 | x86_64 | Vulkan |

Windows 11 26H1 is a tracked compatibility lane for new-device platforms, not a replacement for
25H2 or 24H2. Linux arm64 and Windows arm64 remain tracked platform gaps until Superi declares them
supported. macOS arm64 and x86_64 both remain required while the listed macOS releases support the
architecture.

The version choices reflect the vendor state at the snapshot date. Microsoft identifies Windows 11
24H2 and 25H2 as the broad-deployment releases and 26H1 as a new-device release. Ubuntu lists 26.04,
24.04, and 22.04 as supported LTS releases. Apple publishes macOS 26, 15, and 14 updates and macOS
26 compatibility for both Apple silicon and selected Intel Macs. GitHub currently offers hosted
images for the named macOS and Ubuntu generations, plus Windows Server build images, but hosted
virtual machines are used only for automated build lanes.

Primary references:

- [GitHub-hosted runner images and hardware](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Windows 11 release information](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information)
- [Ubuntu releases](https://wiki.ubuntu.com/Releases)
- [Apple macOS versions](https://support.apple.com/en-us/109033)
- [Apple macOS 26 compatibility](https://support.apple.com/en-us/122867)
- [wgpu supported native backends](https://github.com/gfx-rs/wgpu#supported-platforms)

## Suite catalog

Each lane below names suites by these stable identifiers. A report records the exact test commands
and fixture revisions in addition to the identifiers.

| Suite | Required behavior |
| --- | --- |
| `toolchain` | Rust formatting, default workspace build, workspace tests, strict Clippy, and documentation tests |
| `features` | Default configuration and `os-codecs` configuration compile and test where applicable |
| `fixtures` | Deterministic frame, audio, timeline, and project fixtures produce their expected results |
| `malformed` | Malformed project, media metadata, container, OTIO, graph, and API payload fixtures fail safely |
| `gpu` | Adapter selection, shader compilation, upload, graph evaluation, readback tolerance, device loss, and native surface smoke tests run on a real adapter |
| `codecs` | Software codecs and the host codec backend probe, decode, seek, flush, and encode only capabilities actually advertised by that machine |
| `audio` | Real device discovery, channel layout, sample clock, underrun reporting, and A/V synchronization run against a physical audio device |
| `slice-contract` | All eight canonical stages run through the public API consumer with strict fixture, expectation, state, instrumentation, and honest stub evidence |
| `slice` | Import, single-track placement, trim, one graph effect, and export run through the public API and real engine path |
| `performance` | The locked reference workloads record latency, dropped frames, render time, memory, cache state, and hardware tier |
| `soak` | Repeated device loss, interruption recovery, eight-hour interactive operation, and twenty-four-hour headless render run at the required release cadence |

## Automated operating-system lanes

These lanes prove source portability, deterministic CPU behavior, malformed-input handling, and
feature coherence on every pull request. Each lane also runs the canonical fixture validator and
the normalized eight-stage `slice-contract` command directly. They do not claim real GPU, display,
audio, hardware codec, rendered-pixel, or playable-export coverage. Runner image migrations must
update the lane ID and matrix revision in the same change.

| Lane | Host image or client system | Architecture | Suites | Cadence | Blocking |
| --- | --- | --- | --- | --- | --- |
| `ci-macos-26-arm64` | macOS 26 hosted image | arm64 | `toolchain`, `features`, `fixtures`, `malformed`, `slice-contract` | pull request | yes |
| `ci-macos-15-x64` | macOS 15 Intel hosted image | x86_64 | `toolchain`, `features`, `fixtures`, `malformed`, `slice-contract` | pull request | yes |
| `ci-windows-2025-x64` | Windows Server 2025 hosted image | x86_64 | `toolchain`, `features`, `fixtures`, `malformed`, `slice-contract` | pull request | yes |
| `ci-ubuntu-26-x64` | Ubuntu 26.04 hosted image | x86_64 | `toolchain`, `features`, `fixtures`, `malformed`, `slice-contract` | pull request after preview graduation | yes after graduation |
| `ci-ubuntu-24-x64` | Ubuntu 24.04 hosted image | x86_64 | `toolchain`, `features`, `fixtures`, `malformed`, `slice-contract` | pull request | yes |
| `ci-ubuntu-22-x64` | Ubuntu 22.04 hosted image | x86_64 | `toolchain`, `features`, `fixtures`, `malformed`, `slice-contract` | weekly and release | yes for release |

Until the Ubuntu 26.04 hosted image leaves preview, that lane runs informationally and Ubuntu 24.04
is the blocking Linux pull-request lane. Windows Server compilation never replaces a Windows 11
client hardware lane.

## Physical hardware lanes

The hardware inventory may replace a device with an equivalent or lower capability device in the
same row. A higher capability device cannot replace a baseline row because it would stop exercising
the constrained path. Every physical lane uses a clean machine or a reproducibly restored image,
the native wgpu backend, a physical display path, and a physical audio endpoint.

| Lane | Client system | Required hardware class | Suites | Cadence | Blocking |
| --- | --- | --- | --- | --- | --- |
| `hw-macos-baseline` | macOS 14 on Apple silicon | M1 family, integrated GPU, 16 GB unified memory | `gpu`, `codecs`, `audio`, `slice`, `performance` | nightly and release | yes for release |
| `hw-macos-current` | macOS 26 on Apple silicon | M4 Pro family or newer, integrated GPU, at least 24 GB unified memory | `gpu`, `codecs`, `audio`, `slice`, `performance`, `soak` | weekly and release | yes for release |
| `hw-macos-intel` | macOS 15 or 26 | supported Intel Mac with AMD discrete GPU | `gpu`, `codecs`, `audio`, `slice` | weekly and release | yes for release |
| `hw-windows-integrated` | Windows 11 24H2 | Intel or AMD integrated GPU, 16 GB system memory | `gpu`, `codecs`, `audio`, `slice`, `performance` | nightly and release | yes for release |
| `hw-windows-nvidia` | Windows 11 25H2 | NVIDIA discrete GPU, 32 GB system memory | `gpu`, `codecs`, `audio`, `slice`, `performance`, `soak` | weekly and release | yes for release |
| `hw-windows-amd` | Windows 11 25H2 | AMD discrete GPU, 32 GB system memory | `gpu`, `codecs`, `audio`, `slice`, `performance` | weekly and release | yes for release |
| `hw-windows-26h1` | Windows 11 26H1 | supported new-device system | `gpu`, `codecs`, `audio`, `slice` | weekly | informational until promoted |
| `hw-linux-intel` | Ubuntu 24.04 LTS | Intel integrated or Arc GPU with Vulkan and VA-API drivers, 16 GB system memory | `gpu`, `codecs`, `audio`, `slice`, `performance` | nightly and release | yes for release |
| `hw-linux-amd` | Ubuntu 26.04 LTS | AMD integrated or discrete GPU with Vulkan and VA-API drivers, 32 GB system memory | `gpu`, `codecs`, `audio`, `slice`, `performance`, `soak` | weekly and release | yes for release |
| `hw-linux-nvidia` | Ubuntu 24.04 LTS | NVIDIA discrete GPU with the supported proprietary Vulkan driver, 32 GB system memory | `gpu`, `audio`, `slice`, `performance` | weekly and release | yes for release |

Linux hardware codec results are capability driven. A driver that does not advertise a required
VA-API profile produces an explicit unavailable result, not a software result relabeled as hardware.
The NVIDIA lane does not claim Linux VA-API codec coverage unless its installed driver stack exposes
and passes the same capability probes.

## Required coverage rules

1. Every pull request must pass all currently blocking automated lanes before merge.
2. Changes to GPU, codec, audio, surface, platform FFI, or concurrency code must run the affected
   physical lane before release. A maintainer may require it before merge when the risk is direct.
3. The `fixtures`, `malformed`, `slice-contract`, and `slice` suites use one fixture revision across
   lanes. A platform exception must be encoded as an explicit tolerance or capability expectation,
   never a different semantic fixture.
4. Same-build, same-backend output must be deterministic. Cross-backend golden comparisons use the
   documented node tolerance and the general normalized absolute-error ceiling from the Phase 0
   contract.
5. `os-codecs` tests first record discovered capabilities. They exercise only advertised operations,
   but the absence of an expected capability is still a reported platform gap.
6. A virtual, software-rendered, remote desktop, or headless adapter result cannot satisfy a physical
   hardware lane. Software adapters may provide extra reference evidence under a separate lane ID.
7. A release candidate requires fresh results for every release-blocking physical lane. Missing,
   skipped, or stale results are platform gaps and block claims of complete cross-platform support.
8. A suite that has not been implemented reports a platform gap. Planned coverage cannot count as a
   pass and cannot be replaced by a mocked success result.

## Result record

Every lane emits a structured report and preserves its raw logs. At minimum, the record contains:

- matrix revision, lane ID, suite IDs, pass, fail, skip, and gap status;
- commit SHA, dirty state, build profile, Rust version, Cargo lockfile digest, and enabled features;
- fixture manifest revision, reference project and media IDs, and expected-output revision;
- operating-system name, edition, exact version and build, kernel, and architecture;
- CPU model, logical core count, memory capacity, and declared hardware tier;
- GPU vendor, model, device ID, native backend, driver version, adapter limits, and display path;
- audio device and driver, sample rate, channel layout, and buffer configuration;
- codec backend identity, discovered operations, acceleration mode, and driver or framework version;
- cache state, cache size, cold or warm status, and temporary-storage medium;
- test start and end timestamps, duration, retries, random seed, and artifact links;
- exact failure, warning, skip, and platform-gap reasons.

A retry does not erase the first result. Flaky passes retain the failed attempt and retry count. A
skip without a declared capability reason is a gap.

## Maintenance

Review this matrix at least quarterly and whenever a vendor ends support, a runner label changes, a
new CPU architecture becomes supported, wgpu changes a backend contract, or a platform codec path is
added. Changes to required systems, suites, or hardware classes increment the matrix revision.
Historical reports retain the revision they used so results remain interpretable after the matrix
changes.
