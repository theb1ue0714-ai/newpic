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

for path in \
  AGENTS.md \
  DESIGN.md \
  README.md \
  LICENSE \
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

require_text DESIGN.md 'https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/raycast/DESIGN.md' 'the Raycast DESIGN.md source URL'
require_text DESIGN.md 'MIT License' 'the source license'
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

if (( failures > 0 )); then
  printf 'Repository contract failed with %d issue(s).\n' "$failures" >&2
  exit 1
fi

printf 'Repository contract passed.\n'
