# NewPic Repository Instructions

These instructions apply to every file and task in this repository unless a
more specific nested `AGENTS.md` explicitly overrides them.

## Required references

Before changing code, tests, UI, documentation, build tooling, or research
notes, read and follow:

1. `/home/mint/AGENT.md` for the global frontend and external-reference rules.
2. The root [`DESIGN.md`](DESIGN.md) for NewPic visual and accessibility rules.
3. The approved product specification at
   [`docs/superpowers/specs/2026-10-22-newpic-design.md`](docs/superpowers/specs/2026-10-22-newpic-design.md).
4. The approved implementation plan at
   [`docs/plans/2026-10-22-newpic-mvp-implementation.md`](docs/plans/2026-10-22-newpic-mvp-implementation.md).
5. The clean-room policy at
   [`docs/architecture/clean-room-policy.md`](docs/architecture/clean-room-policy.md).

Resolve conflicts in that order after system and user instructions. Do not
expand Phase 1 scope without an approved specification update.

## Engineering workflow

- Use test-driven development for production behavior: add a focused failing
  test, confirm the expected RED failure, implement the smallest change, and
  confirm GREEN before refactoring.
- Preserve platform-neutral contracts in shared crates. Keep platform API,
  permission, and capability details inside the relevant adapter.
- Do not silently weaken Windows, macOS, Linux X11, or Linux Wayland behavior.
  Report unsupported capabilities explicitly through diagnostics.
- Treat captured pixels, clipboard contents, paths, and diagnostic data as
  private local data.

## Verification gates

These verification gates apply to every completed implementation task.

Before claiming a task complete:

1. Run the tests and static checks named by the implementation plan task.
2. Run `bash scripts/check-repo-contract.sh` when repository instructions,
   design guidance, metadata, research policy, or documentation changes.
3. Inspect `git diff --check` and `git status --short`.
4. Report any platform checks that could not run on the current host; never
   describe an unexecuted check as passing.

Commits must be narrowly scoped to the active plan task and must not include
REA captures, recovered binaries, secrets, signing material, or proprietary
PicPick assets.
