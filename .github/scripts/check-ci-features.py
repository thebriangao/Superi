#!/usr/bin/env python3
"""Validate the hosted default and os-codecs CI contract."""

from pathlib import Path
import sys


WORKFLOW = Path(__file__).resolve().parents[1] / "workflows" / "ci.yml"


def require(text: str, needle: str, description: str) -> None:
    if needle not in text:
        raise ValueError(f"missing {description}: {needle}")


def lane_block(text: str, lane: str) -> str:
    marker = f"          - lane: {lane}\n"
    start = text.find(marker)
    if start < 0:
        raise ValueError(f"missing matrix lane: {lane}")
    next_lane = text.find("          - lane:", start + len(marker))
    end = next_lane if next_lane >= 0 else text.find("\n    env:", start)
    if end < 0:
        raise ValueError(f"cannot determine matrix lane boundary: {lane}")
    return text[start:end]


def main() -> int:
    text = WORKFLOW.read_text(encoding="utf-8")
    if "--all-features" in text:
        raise ValueError("the os-codecs lane must not broaden into all features")

    expected = {
        "ci-macos-26-arm64": "true",
        "ci-macos-15-x64": "true",
        "ci-windows-2025-x64": "true",
        "ci-ubuntu-26-x64": "true",
        "ci-ubuntu-24-x64": "false",
    }
    for lane, enabled in expected.items():
        require(lane_block(text, lane), f"            os_codecs: {enabled}\n", f"{lane} os-codecs policy")

    require(text, "run: python3 ../.github/scripts/check-ci-features.py", "feature contract step")
    require(text, "if: matrix.os_codecs", "os-codecs step gate")
    require(
        text,
        "run: cargo build --locked -p superi-cli --features os-codecs",
        "real CLI os-codecs consumer build",
    )
    require(
        text,
        "run: cargo test --locked -p superi-engine -p superi-api --features superi-engine/os-codecs,superi-api/os-codecs",
        "engine and API os-codecs consumer tests",
    )

    if text.count("run: cargo build --workspace --locked") != 2:
        raise ValueError("both CI jobs must retain the locked default workspace build")
    if "matrix.os_codecs" in text[text.find("  extended-linux-build:") :]:
        raise ValueError("the Ubuntu 22 extended job must remain default-only")

    print("validated default and os-codecs CI feature coverage")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"ci feature contract failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
