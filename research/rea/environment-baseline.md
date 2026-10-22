# REA Environment Baseline

- Recorded at: `2026-10-22T12:51:22+09:00`
- Host: `mintOS`, Linux x86_64
- Distribution: Linux Mint 22.3 (Zena), Ubuntu 24.04 (`noble`) base
- Kernel: `6.17.0-20-generic`
- Purpose: establish the pre-install environment for the pinned REA workflow used by NewPic research.

## Pre-install commands and output

```text
$ node --version
v24.15.0

$ npm --version
11.12.1

$ rustc --version
rustc 1.99.0 (b940084d7 2026-09-28)

$ command -v ps
/usr/bin/ps

$ npm view rea-agents version
6.7.0

$ command -v rea
(no output; REA was not installed or available on PATH)

$ npm prefix --global
/home/mint/.nvm/versions/node/v24.15.0
```

The environment matches the implementation plan's pinned starting versions for Node and Rust, and the npm registry reports the approved `rea-agents` version `6.7.0`.

## Native reverse-engineering tools

Hopper and Ghidra are not required for this phase. NewPic's REA work targets a JavaScript/desktop application workflow and records only externally observable behavior and sanitized configuration facts. It does not inspect, decompile, or copy native proprietary binaries. If a later, separately approved investigation genuinely requires native binary analysis, its tooling, legal scope, and evidence-handling rules must be reviewed before installation.

## Installation and registration verification

- Installed package: `rea-agents@6.7.0`
- `rea --version`: `6.7.0`
- Dry-run result: valid JSON; planned changes were limited to the Codex MCP registration and the bundled REA workflow skill.
- Setup result: exit `0`, status `ready`, actions `configured_codex` and `installed_skill`.
- Doctor result: exit `0`, `healthy: true`, Node and `registration:codex` scope checks passed.
- Identity: CLI package `6.7.0`; bundled skill version `42` with `207` tools; Codex registration state `aligned`.
- Analysis engines: absent and reported as informational `missing_analysis_engine` checks; not required for the approved JavaScript/desktop behavior workflow.
- Active-session discovery: the current Codex process does not expose the newly registered REA MCP server. Restart Codex before attempting MCP calls; tool availability must be confirmed in that restarted process rather than inferred from configuration.
