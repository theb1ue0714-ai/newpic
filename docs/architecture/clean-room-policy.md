# NewPic Clean-Room Research Policy

## Purpose

NewPic is an independent implementation of the approved screenshot workflow.
Research may establish externally observable behavior, compatibility
constraints, and testable requirements. It must not transfer PicPick code,
creative assets, or proprietary implementation details into NewPic.

This policy applies to every contributor, agent, local analysis tool, CI job,
research note, patch, commit, release artifact, and review.

## Approved inputs

Implementation work may rely on:

- the approved NewPic specification and implementation plan
- sanitized behavioral observations recorded under `research/`
- official operating-system and dependency documentation
- independently created tests and fixtures
- publicly documented file formats, platform APIs, and interoperability facts

A PicPick installer or installation may be examined only when it was obtained
legitimately and only to answer a requirement that cannot be established from
public documentation or black-box use alone.

## Research record requirements

Each behavioral research note must record:

1. target product and exact version
2. acquisition source and cryptographic hash when a binary is involved
3. the question being tested
4. the observable procedure and result
5. confirmed facts separated from inference
6. unresolved questions and the next safe probe

Notes committed to Git must contain sanitized behavior descriptions only.
Screenshots may be described by geometry, state, and result, but proprietary
pixels, embedded resources, dumps, disassembly, or copied UI text must not be
committed.

## Prohibited material

Do not place any of the following in the repository, issue tracker, build
output, generated fixture, documentation, or source comment:

- PicPick source or decompiled/recovered source
- binaries, installer contents, memory dumps, packet captures, or extracted
  resources from PicPick
- PicPick icons, logos, screenshots, artwork, sounds, help text, or branding
- copied implementation-specific strings beyond short functional labels that
  are necessary for interoperability
- reconstructed algorithms based on proprietary internals rather than
  independently observable behavior
- license keys, credentials, personal data, captured user content, or secrets

The `.gitignore` entries for raw REA work are a safeguard, not permission to
store prohibited material inside the repository. Keep any authorized local
analysis inputs outside Git and remove them when they are no longer needed.

## Separation of research and implementation

Researchers produce only sanitized facts and testable behavior notes.
Implementers use the approved specification and those sanitized notes, not
disassembly, decompilation, extracted resources, or proprietary binaries. When
one person or agent performs both roles, it must still write the sanitized note
first and implement from that note rather than copying from raw analysis.

If provenance is unclear, stop and quarantine the material outside the
repository. Do not use it until its lawful source and clean-room suitability
are established.

## UI and naming

NewPic must use its own name, identifiers, copy, layout, icons, and visual
assets. The root `DESIGN.md` is an MIT-licensed external reference with an
explicit NewPic adaptation; it does not authorize copying Raycast or PicPick
brand assets. Public release naming remains provisional until trademark and
distribution-name checks are complete.

## Review gate

Every research-related change must be checked for:

- sanitized observations rather than proprietary artifacts
- complete provenance and hash fields where applicable
- clear fact/inference separation
- no captured pixels, clipboard contents, credentials, or personal data
- no copied branding, icons, screenshots, or implementation details

Suspected contamination blocks the change. Remove the material, document the
incident without reproducing it, and re-derive the requirement from an
approved source before implementation continues.
