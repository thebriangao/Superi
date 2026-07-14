#!/usr/bin/env bash
set -euo pipefail

repository_root="$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repository_root/.github/workflows/dependency-policy.yml"
policy="$repository_root/open/deny.toml"

require_line() {
  local file="$1"
  local line="$2"

  if ! grep -Fqx "$line" "$file"; then
    printf 'missing required line in %s: %s\n' "$file" "$line" >&2
    return 1
  fi
}

require_line "$workflow" 'name: Dependency policy'
require_line "$workflow" '  contents: read'
require_line "$workflow" '        run: bash .github/scripts/check-dependency-policy.sh'
require_line "$workflow" '        uses: EmbarkStudios/cargo-deny-action@v2'
require_line "$workflow" '          manifest-path: open/Cargo.toml'
require_line "$workflow" '          command: check'
require_line "$workflow" '          arguments: --all-features'
require_line "$workflow" '          command-arguments: licenses sources'

require_line "$policy" 'unknown-git = "deny"'
require_line "$policy" 'required-git-spec = "rev"'
require_line "$policy" '    "https://github.com/OxideAV/oxideav-mp3",'

printf 'dependency policy workflow contract ok\n'
