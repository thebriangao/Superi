#!/usr/bin/env python3
"""Discover, shard, hash, and validate Superi codebase maps."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[4]
MAP_ROOT = REPO_ROOT / "docs" / "codebase-map"
MODULE_MAP_ROOT = MAP_ROOT / "modules"
REQUIRED_HEADINGS = (
    "## Purpose and ownership",
    "## Source inventory",
    "## Public surface",
    "## Architecture and data flow",
    "## Dependencies and consumers",
    "## Invariants and operational boundaries",
    "## Tests and verification",
    "## Current status and risks",
    "## Maintenance notes",
)
EXCLUDED_PREFIXES = (
    ".git/",
    "docs/codebase-map/",
    "open/target/",
    "plans/",
)


@dataclass(frozen=True)
class SourceFile:
    path: str
    lines: int
    bytes: int
    text: bool


@dataclass(frozen=True)
class Module:
    module_id: str
    source_paths: tuple[str, ...]
    files: tuple[SourceFile, ...]
    source_hash: str

    @property
    def source_files(self) -> int:
        return len(self.files)

    @property
    def source_lines(self) -> int:
        return sum(item.lines for item in self.files)

    @property
    def map_path(self) -> str:
        return f"docs/codebase-map/modules/{self.module_id}.md"


def run_git(*args: str) -> bytes:
    result = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout


def repository_files() -> list[str]:
    output = run_git("ls-files", "--cached", "--others", "--exclude-standard", "-z")
    paths = []
    for raw in output.split(b"\0"):
        if not raw:
            continue
        path = raw.decode("utf-8")
        if any(path == prefix[:-1] or path.startswith(prefix) for prefix in EXCLUDED_PREFIXES):
            continue
        if (REPO_ROOT / path).is_file():
            paths.append(path)
    return sorted(set(paths))


def module_id_for_path(path: str) -> str:
    parts = Path(path).parts
    if len(parts) >= 3 and parts[:2] == ("open", "crates"):
        return parts[2]
    if len(parts) >= 3 and parts[:2] == ("open", "tools"):
        return f"tool-{parts[2]}"
    return "workspace"


def source_paths_for(module_id: str, paths: list[str]) -> tuple[str, ...]:
    if module_id == "workspace":
        return ("repository files outside open/crates/* and open/tools/*",)
    owned = [path for path in paths if module_id_for_path(path) == module_id]
    roots = sorted({str(Path(path).parent) for path in owned})
    if module_id.startswith("tool-"):
        return (f"open/tools/{module_id.removeprefix('tool-')}",)
    return (f"open/crates/{module_id}",) if owned else tuple(roots)


def inspect_file(path: str) -> SourceFile:
    data = (REPO_ROOT / path).read_bytes()
    text = b"\0" not in data
    if text:
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            text = False
    lines = len(data.splitlines()) if text else 0
    return SourceFile(path=path, lines=lines, bytes=len(data), text=text)


def hash_files(files: tuple[SourceFile, ...]) -> str:
    digest = hashlib.sha256()
    for item in files:
        digest.update(item.path.encode("utf-8"))
        digest.update(b"\0")
        digest.update((REPO_ROOT / item.path).read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def discover_modules() -> dict[str, Module]:
    paths = repository_files()
    grouped: dict[str, list[str]] = {}
    for path in paths:
        grouped.setdefault(module_id_for_path(path), []).append(path)
    modules = {}
    for module_id, owned in sorted(grouped.items()):
        files = tuple(inspect_file(path) for path in sorted(owned))
        modules[module_id] = Module(
            module_id=module_id,
            source_paths=source_paths_for(module_id, paths),
            files=files,
            source_hash=hash_files(files),
        )
    return modules


def require_module(modules: dict[str, Module], module_id: str) -> Module:
    try:
        return modules[module_id]
    except KeyError:
        known = ", ".join(modules)
        raise SystemExit(f"unknown module {module_id!r}; expected one of: {known}")


def module_record(module: Module) -> dict[str, object]:
    return {
        "module_id": module.module_id,
        "source_paths": list(module.source_paths),
        "source_hash": module.source_hash,
        "source_files": module.source_files,
        "source_lines": module.source_lines,
        "map_path": module.map_path,
    }


def command_inventory(args: argparse.Namespace) -> int:
    modules = discover_modules()
    records = [module_record(module) for module in modules.values()]
    print(json.dumps(records, indent=2))
    return 0


def command_files(args: argparse.Namespace) -> int:
    module = require_module(discover_modules(), args.module_id)
    print(json.dumps([asdict(item) for item in module.files], indent=2))
    return 0


def command_hash(args: argparse.Namespace) -> int:
    module = require_module(discover_modules(), args.module_id)
    print(json.dumps(module_record(module), indent=2))
    return 0


def shard_files(module: Module, max_lines: int) -> list[list[SourceFile]]:
    shards: list[list[SourceFile]] = []
    current: list[SourceFile] = []
    current_weight = 0
    for item in module.files:
        weight = max(item.lines, 1)
        if current and current_weight + weight > max_lines:
            shards.append(current)
            current = []
            current_weight = 0
        current.append(item)
        current_weight += weight
    if current:
        shards.append(current)
    return shards


def command_shards(args: argparse.Namespace) -> int:
    module = require_module(discover_modules(), args.module_id)
    payload = []
    for index, items in enumerate(shard_files(module, args.max_lines), start=1):
        payload.append(
            {
                "shard_id": f"{index:03d}",
                "module_id": module.module_id,
                "lines": sum(item.lines for item in items),
                "bytes": sum(item.bytes for item in items),
                "files": [asdict(item) for item in items],
                "note_path": (
                    f"plans/codebase-mapping/{module.module_id}/shards/{index:03d}.md"
                ),
            }
        )
    print(json.dumps(payload, indent=2))
    return 0


def changed_paths(base: str) -> list[str]:
    output = run_git("diff", "--name-only", "-z", base, "--")
    changed = {raw.decode("utf-8") for raw in output.split(b"\0") if raw}
    untracked = run_git("ls-files", "--others", "--exclude-standard", "-z")
    changed.update(raw.decode("utf-8") for raw in untracked.split(b"\0") if raw)
    return sorted(
        path
        for path in changed
        if not any(path == prefix[:-1] or path.startswith(prefix) for prefix in EXCLUDED_PREFIXES)
    )


def command_changed(args: argparse.Namespace) -> int:
    paths = changed_paths(args.base)
    grouped: dict[str, list[str]] = {}
    for path in paths:
        grouped.setdefault(module_id_for_path(path), []).append(path)
    payload = [
        {"module_id": module_id, "paths": owned}
        for module_id, owned in sorted(grouped.items())
    ]
    print(json.dumps(payload, indent=2))
    return 0


def parse_frontmatter(text: str, map_path: str) -> tuple[dict[str, object], str, list[str]]:
    errors = []
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        return {}, text, [f"frontmatter must begin {map_path}"]
    try:
        closing = lines.index("---", 1)
    except ValueError:
        return {}, text, [f"frontmatter is not closed in {map_path}"]

    metadata: dict[str, object] = {}
    active_list: str | None = None
    for line in lines[1:closing]:
        if line.startswith("  - "):
            if active_list is None:
                errors.append(f"orphan frontmatter list item in {map_path}")
                continue
            value = line[4:].strip()
            items = metadata.setdefault(active_list, [])
            if not isinstance(items, list):
                errors.append(f"mixed scalar and list metadata for {active_list} in {map_path}")
                continue
            items.append(value)
            continue
        match = re.fullmatch(r"([a-z_]+):(?:\s*(.*))?", line)
        if not match:
            errors.append(f"malformed frontmatter line in {map_path}: {line!r}")
            active_list = None
            continue
        key, value = match.groups()
        if key in metadata:
            errors.append(f"duplicate frontmatter key {key} in {map_path}")
        if value:
            metadata[key] = value.strip()
            active_list = None
        else:
            metadata[key] = []
            active_list = key

    body = "\n".join(lines[closing + 1 :])
    return metadata, body, errors


def section_text(body: str, heading: str) -> str:
    lines = body.splitlines()
    try:
        start = lines.index(heading) + 1
    except ValueError:
        return ""
    end = next(
        (index for index in range(start, len(lines)) if lines[index].startswith("## ")),
        len(lines),
    )
    return "\n".join(lines[start:end])


def validate_module_map(module: Module) -> list[str]:
    errors = []
    path = REPO_ROOT / module.map_path
    if not path.exists():
        return [f"missing map: {module.map_path}"]
    text = path.read_text(encoding="utf-8")
    if "\u2013" in text or "\u2014" in text:
        errors.append(f"forbidden Unicode dash in {module.map_path}")
    metadata, body, frontmatter_errors = parse_frontmatter(text, module.map_path)
    errors.extend(frontmatter_errors)
    if metadata.get("module_id") != module.module_id:
        errors.append(f"wrong module_id in {module.map_path}")
    if metadata.get("source_paths") != list(module.source_paths):
        errors.append(f"wrong source_paths in {module.map_path}")
    if metadata.get("source_hash") != module.source_hash:
        errors.append(f"stale source_hash in {module.map_path}")
    if metadata.get("source_files") != str(module.source_files):
        errors.append(f"stale source_files in {module.map_path}")
    mapped_at_commit = metadata.get("mapped_at_commit")
    if not isinstance(mapped_at_commit, str) or not re.fullmatch(
        r"(?:[0-9a-f]{40}|working-tree)", mapped_at_commit
    ):
        errors.append(f"invalid mapped_at_commit in {module.map_path}")
    for heading in REQUIRED_HEADINGS:
        if sum(line == heading for line in body.splitlines()) != 1:
            errors.append(f"expected one {heading!r} in {module.map_path}")
    inventory = section_text(body, "## Source inventory")
    for item in module.files:
        entry = rf"(?m)^- `{re.escape(item.path)}`(?:\s|:)"
        if len(re.findall(entry, inventory)) != 1:
            errors.append(f"missing inventory path {item.path} in {module.map_path}")
    return errors


def validate_index(modules: dict[str, Module]) -> list[str]:
    path = MAP_ROOT / "index.md"
    if not path.exists():
        return ["missing map: docs/codebase-map/index.md"]
    text = path.read_text(encoding="utf-8")
    errors = []
    if "\u2013" in text or "\u2014" in text:
        errors.append("forbidden Unicode dash in docs/codebase-map/index.md")
    targets = re.findall(r"\[[^\]]+\]\(([^)]+)\)", text)
    for module in modules.values():
        link = f"modules/{module.module_id}.md"
        if targets.count(link) != 1:
            errors.append(f"expected one index link: {link}")
    for target in targets:
        if target.startswith(("http://", "https://", "#")):
            continue
        target_path = (MAP_ROOT / target.split("#", 1)[0]).resolve()
        if not target_path.is_file():
            errors.append(f"broken index link: {target}")
    return errors


def command_validate(args: argparse.Namespace) -> int:
    modules = discover_modules()
    errors = []
    expected_maps = {(REPO_ROOT / module.map_path).resolve() for module in modules.values()}
    actual_maps = {path.resolve() for path in MODULE_MAP_ROOT.glob("*.md")}
    for extra in sorted(actual_maps - expected_maps):
        errors.append(f"unexpected module map: {extra.relative_to(REPO_ROOT)}")
    for module in modules.values():
        errors.extend(validate_module_map(module))
    errors.extend(validate_index(modules))
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        print(f"validation failed with {len(errors)} error(s)")
        return 1
    print(f"validated {len(modules)} module maps and the global index")
    return 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)

    inventory = commands.add_parser("inventory", help="print all module records as JSON")
    inventory.set_defaults(handler=command_inventory)

    files = commands.add_parser("files", help="print one module's complete file inventory")
    files.add_argument("module_id")
    files.set_defaults(handler=command_files)

    module_hash = commands.add_parser("hash", help="print one module's current hash record")
    module_hash.add_argument("module_id")
    module_hash.set_defaults(handler=command_hash)

    shards = commands.add_parser("shards", help="partition a module into whole-file shards")
    shards.add_argument("module_id")
    shards.add_argument("--max-lines", type=int, default=4000)
    shards.set_defaults(handler=command_shards)

    changed = commands.add_parser("changed", help="print modules changed from a revision")
    changed.add_argument("--base", default="HEAD")
    changed.set_defaults(handler=command_changed)

    validate = commands.add_parser("validate", help="verify all maps against current source")
    validate.set_defaults(handler=command_validate)
    return root


def main() -> int:
    args = parser().parse_args()
    return args.handler(args)


if __name__ == "__main__":
    sys.exit(main())
