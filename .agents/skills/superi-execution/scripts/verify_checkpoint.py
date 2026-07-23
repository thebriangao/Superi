#!/usr/bin/env python3
"""Select and run the final local verification gates for one checkpoint diff."""

from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass
import json
from pathlib import Path
import platform
import shlex
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[4]
OPEN = ROOT / "open"
API_CLIENT_CONTRACT = ROOT / "ci" / "api-client-contract"
EDITORIAL_CONTRACTS = OPEN / "bindings" / "typescript" / "editorial-contracts"


@dataclass(frozen=True)
class Gate:
    name: str
    command: tuple[str, ...]
    cwd: Path = ROOT


def capture(*command: str) -> str:
    return subprocess.run(
        command,
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    ).stdout


def changed_paths(base: str) -> list[str]:
    capture("git", "rev-parse", "--verify", f"{base}^{{commit}}")
    sources = [
        capture("git", "diff", "--name-only", "--diff-filter=ACDMRTUXB", f"{base}...HEAD"),
        capture("git", "diff", "--name-only", "--diff-filter=ACDMRTUXB"),
        capture("git", "diff", "--cached", "--name-only", "--diff-filter=ACDMRTUXB"),
        capture("git", "ls-files", "--others", "--exclude-standard"),
    ]
    return sorted({line for source in sources for line in source.splitlines() if line})


def has_prefix(paths: list[str], *prefixes: str) -> bool:
    return any(path.startswith(prefixes) for path in paths)


def validate_changed_text(paths: list[str]) -> None:
    for relative in paths:
        path = ROOT / relative
        if not path.is_file():
            continue
        if path.suffix == ".py":
            ast.parse(path.read_text(encoding="utf-8"), filename=relative)
        elif path.suffix == ".json":
            json.loads(path.read_text(encoding="utf-8"))


def select_gates(paths: list[str], full: bool, temporary: Path) -> list[Gate]:
    gates = [
        Gate(
            "codebase map validation",
            (
                "python3",
                ".agents/skills/superi-mapping/scripts/codebase_maps.py",
                "validate",
            ),
        )
    ]

    shell_files = tuple(path for path in paths if path.endswith(".sh"))
    if shell_files:
        gates.append(Gate("changed shell syntax", ("bash", "-n", *shell_files)))

    ci_contract = full or any(
        path in {".github/workflows/ci.yml", ".github/scripts/check-ci-features.py"}
        for path in paths
    )
    dependency_policy = full or any(
        path.endswith(("Cargo.toml", "Cargo.lock"))
        or path == "open/deny.toml"
        or path.startswith("open/tools/superi-dependency-check/")
        or path in {
            ".github/workflows/dependency-policy.yml",
            ".github/scripts/check-dependency-policy.sh",
        }
        for path in paths
    )
    typescript_contracts = (
        full
        or has_prefix(
            paths,
            "ci/api-client-contract/",
            "open/bindings/typescript/editorial-contracts/",
        )
        or any(path == ".github/workflows/typescript-contracts.yml" for path in paths)
    )
    rust = full or any(
        path.startswith("open/") and not path.endswith((".md", ".txt")) for path in paths
    )
    os_codecs = full or has_prefix(
        paths,
        "open/crates/superi-api/",
        "open/crates/superi-cli/",
        "open/crates/superi-codecs-platform/",
        "open/crates/superi-codecs-rs/",
        "open/crates/superi-codecs-vendor/",
        "open/crates/superi-engine/",
        "open/crates/superi-media-io/",
    ) or any(path in {"open/Cargo.toml", "open/Cargo.lock"} for path in paths)

    if ci_contract or rust:
        gates.append(
            Gate(
                "CI feature contract",
                ("python3", ".github/scripts/check-ci-features.py"),
            )
        )

    if dependency_policy:
        gates.extend(
            [
                Gate(
                    "dependency workflow contract",
                    ("bash", ".github/scripts/check-dependency-policy.sh"),
                ),
                Gate(
                    "dependency licenses and sources",
                    (
                        "cargo",
                        "deny",
                        "--manifest-path",
                        "open/Cargo.toml",
                        "--all-features",
                        "check",
                        "licenses",
                        "sources",
                    ),
                ),
            ]
        )

    if rust:
        workspace_test = ["cargo", "test", "--workspace", "--locked"]
        if platform.system() == "Darwin":
            workspace_test.extend(
                [
                    "--",
                    "--skip",
                    "h264_native_roundtrip_preserves_timing_and_external_frame_ownership",
                    "--skip",
                    "hevc_and_every_advertised_prores_profile_complete_native_roundtrips",
                    "--skip",
                    "aac_audio_converter_roundtrip_preserves_sample_timing_and_channel_layout",
                ]
            )
        gates.extend(
            [
                Gate(
                    "open product boundary",
                    ("cargo", "run", "--locked", "-p", "superi-boundary-tool", "--", "check", "."),
                    OPEN,
                ),
                Gate("workspace formatting", ("cargo", "fmt", "--all", "--", "--check"), OPEN),
                Gate("workspace build", ("cargo", "build", "--workspace", "--locked"), OPEN),
                Gate("workspace tests", tuple(workspace_test), OPEN),
                Gate(
                    "workspace lint",
                    ("cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"),
                    OPEN,
                ),
                Gate(
                    "workspace documentation tests",
                    ("cargo", "test", "--workspace", "--doc", "--locked"),
                    OPEN,
                ),
                Gate(
                    "dependency direction",
                    ("cargo", "run", "--locked", "-p", "superi-dependency-check"),
                    OPEN,
                ),
                Gate(
                    "generated API binding drift",
                    ("cargo", "run", "--locked", "-p", "superi-api-bindings", "--", "check"),
                    OPEN,
                ),
                Gate(
                    "canonical fixtures",
                    ("cargo", "run", "--locked", "-p", "superi-fixture-tool", "--", "check", "test-fixtures"),
                    OPEN,
                ),
                Gate(
                    "canonical editorial slice",
                    (
                        "cargo",
                        "run",
                        "--locked",
                        "-p",
                        "superi-cli",
                        "--",
                        "slice",
                        "run",
                        "--scenario",
                        "superi.slice.canonical.v1",
                        "--artifact-dir",
                        str(temporary / "slice-artifacts"),
                        "--report",
                        str(temporary / "slice-report.json"),
                    ),
                    OPEN,
                ),
            ]
        )

    if os_codecs:
        gates.extend(
            [
                Gate(
                    "operating system codec build",
                    ("cargo", "build", "--locked", "-p", "superi-cli", "--features", "os-codecs"),
                    OPEN,
                ),
                Gate(
                    "operating system codec CLI tests",
                    ("cargo", "test", "--locked", "-p", "superi-cli", "--features", "os-codecs"),
                    OPEN,
                ),
                Gate(
                    "operating system codec consumer tests",
                    (
                        "cargo",
                        "test",
                        "--locked",
                        "-p",
                        "superi-engine",
                        "-p",
                        "superi-api",
                        "--features",
                        "superi-engine/os-codecs,superi-api/os-codecs",
                    ),
                    OPEN,
                ),
            ]
        )

    if typescript_contracts:
        gates.extend(
            [
                Gate("API client locked install", ("npm", "ci"), API_CLIENT_CONTRACT),
                Gate(
                    "API client typecheck",
                    ("npm", "run", "typecheck"),
                    API_CLIENT_CONTRACT,
                ),
                Gate("API client contract tests", ("npm", "test"), API_CLIENT_CONTRACT),
                Gate(
                    "editorial contracts locked install",
                    ("npm", "ci"),
                    EDITORIAL_CONTRACTS,
                ),
                Gate(
                    "editorial contracts typecheck",
                    ("npm", "run", "typecheck"),
                    EDITORIAL_CONTRACTS,
                ),
                Gate(
                    "editorial contract tests",
                    ("npm", "test"),
                    EDITORIAL_CONTRACTS,
                ),
            ]
        )

    return gates


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="synchronized commit before checkpoint work")
    parser.add_argument("--full", action="store_true", help="run every supported verification gate")
    parser.add_argument("--dry-run", action="store_true", help="print selected gates without running them")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if Path.cwd().resolve() != ROOT:
        print(f"run from repository root: {ROOT}", file=sys.stderr)
        return 2

    try:
        paths = changed_paths(args.base)
        validate_changed_text(paths)
    except (OSError, UnicodeError, subprocess.CalledProcessError, SyntaxError, json.JSONDecodeError) as error:
        print(f"cannot prepare verification: {error}", file=sys.stderr)
        return 1

    print(f"verification base: {args.base}")
    print(f"changed paths: {len(paths)}")
    for path in paths:
        print(f"  {path}")

    with tempfile.TemporaryDirectory(prefix="superi-checkpoint-") as temporary:
        gates = select_gates(paths, args.full, Path(temporary))
        for number, gate in enumerate(gates, start=1):
            rendered = shlex.join(gate.command)
            relative = gate.cwd.relative_to(ROOT) if gate.cwd != ROOT else Path(".")
            print(f"[{number}/{len(gates)}] {gate.name}: (cd {relative} && {rendered})")
            if not args.dry_run:
                subprocess.run(gate.command, cwd=gate.cwd, check=True)

    print("checkpoint verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
