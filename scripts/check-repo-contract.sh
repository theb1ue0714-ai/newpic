#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failures=0

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  failures=$((failures + 1))
}

require_file() {
  local path="$1"
  [[ -f "$repo_root/$path" ]] || fail "missing required file: $path"
}

require_text() {
  local path="$1"
  local needle="$2"
  local description="$3"

  if [[ ! -f "$repo_root/$path" ]]; then
    fail "cannot check $description because $path is missing"
  elif ! grep -Fq -- "$needle" "$repo_root/$path"; then
    fail "$path must reference $description"
  fi
}

forbid_text() {
  local path="$1"
  local needle="$2"
  local description="$3"

  if [[ -f "$repo_root/$path" ]] && grep -Fq -- "$needle" "$repo_root/$path"; then
    fail "$path must not reference $description"
  fi
}

require_order() {
  local path="$1"
  local first="$2"
  local second="$3"
  local description="$4"
  local first_line second_line

  if [[ ! -f "$repo_root/$path" ]]; then
    fail "cannot check $description because $path is missing"
    return
  fi

  first_line="$(grep -nFm1 -- "$first" "$repo_root/$path" | cut -d: -f1 || true)"
  second_line="$(grep -nFm1 -- "$second" "$repo_root/$path" | cut -d: -f1 || true)"
  if [[ -z "$first_line" || -z "$second_line" || "$first_line" -ge "$second_line" ]]; then
    fail "$path must place $description"
  fi
}

for path in \
  AGENTS.md \
  DESIGN.md \
  README.md \
  LICENSE \
  THIRD_PARTY_NOTICES.md \
  .gitignore \
  docs/architecture/clean-room-policy.md; do
  require_file "$path"
done

require_text AGENTS.md '/home/mint/AGENT.md' '/home/mint/AGENT.md'
require_text AGENTS.md 'DESIGN.md' 'the root visual design system'
require_text AGENTS.md 'docs/superpowers/specs/2026-10-22-newpic-design.md' 'the approved NewPic specification'
require_text AGENTS.md 'docs/plans/2026-10-22-newpic-mvp-implementation.md' 'the approved MVP implementation plan'
require_text AGENTS.md 'docs/architecture/clean-room-policy.md' 'the clean-room policy'
require_text AGENTS.md 'test-driven' 'test-driven development'
require_text AGENTS.md 'verification' 'verification gates'
require_text AGENTS.md 'Current system, developer, user, and repository instructions' 'current instruction precedence'
require_order AGENTS.md 'Current system, developer, user, and repository instructions' 'docs/superpowers/specs/2026-10-22-newpic-design.md' 'current instructions before the approved specification'
require_order AGENTS.md 'docs/superpowers/specs/2026-10-22-newpic-design.md' 'docs/architecture/clean-room-policy.md' 'the approved specification before the clean-room policy'
require_order AGENTS.md 'docs/architecture/clean-room-policy.md' 'DESIGN.md' 'the clean-room policy before the derived visual design'

require_text README.md 'Node.js 24.15' 'the pinned Node.js 24.15-compatible toolchain'
require_text README.md 'npm 11' 'npm 11'
require_text README.md 'npm workspaces' 'npm workspaces'
require_text README.md 'package-lock.json' 'the npm lockfile'
require_text README.md 'npm install' 'the npm installation command'
forbid_text README.md 'pnpm' 'pnpm as the package manager'

require_text DESIGN.md 'https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/raycast/DESIGN.md' 'the Raycast DESIGN.md source URL'
require_text DESIGN.md 'MIT License' 'the source license'
require_text DESIGN.md '属于: >-' 'YAML-compatible folded source metadata'
require_text DESIGN.md 'YAML compatibility adaptation' 'the documented front-matter normalization'
require_text DESIGN.md 'NewPic Adaptation' 'the NewPic adaptation section'
require_text DESIGN.md 'utility-window' 'the utility-window adaptation'
require_text DESIGN.md 'system font' 'the system-font adaptation'
require_text DESIGN.md 'light and dark' 'light and dark token pairs'
require_text DESIGN.md '8px' 'the standard radius'
require_text DESIGN.md 'hairline' 'hairline borders'
require_text DESIGN.md 'blue capture accent' 'the restrained capture accent'
require_text DESIGN.md 'decorative gradient' 'the gradient restriction'
require_text DESIGN.md '44px' 'minimum interactive target guidance'
require_text DESIGN.md 'prefers-reduced-motion' 'reduced-motion support'
require_text DESIGN.md 'WCAG AA' 'contrast requirements'

require_text THIRD_PARTY_NOTICES.md 'Raycast DESIGN.md reference' 'the incorporated design reference'
require_text THIRD_PARTY_NOTICES.md 'Copyright (c) 2026 VoltAgent' 'the upstream copyright notice'
require_text THIRD_PARTY_NOTICES.md 'Permission is hereby granted, free of charge' 'the MIT permission grant'
require_text THIRD_PARTY_NOTICES.md 'THE SOFTWARE IS PROVIDED "AS IS"' 'the MIT warranty disclaimer'

if (( failures > 0 )); then
  printf 'Repository contract failed with %d issue(s).\n' "$failures" >&2
  exit 1
fi

printf 'Repository contract passed.\n'
