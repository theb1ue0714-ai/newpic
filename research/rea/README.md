# REA Research Evidence Rules

This directory contains sanitized research evidence used to implement NewPic independently. REA output is evidence about externally observable behavior, not source material to copy.

## Required record fields

Every investigation record must include all of the following:

1. **Target source URL** — the exact public download page, documentation page, or other authorized source examined.
2. **Target version** — the precise application or artifact version. If it cannot be established, record `unknown` and explain why.
3. **SHA-256** — the SHA-256 digest of every locally examined artifact. Never rely on a filename alone.
4. **Confirmed facts** — directly observed behavior, inputs, outputs, and environment details. State the evidence used for each fact.
5. **Inference** — conclusions derived from observations, clearly labeled as inference rather than fact.
6. **Unknowns** — unanswered questions, ambiguous behavior, and untested platform cases.
7. **Clean-room notes** — how the observation was converted into an independent NewPic requirement without carrying over protected implementation details or assets.

Use a structure equivalent to:

```markdown
# Investigation title

- Target source URL:
- Target version:
- Artifact SHA-256:
- Observation environment:

## Confirmed facts

## Inference

## Unknowns

## Clean-room notes
```

## Clean-room prohibitions

- Do not copy, translate, reconstruct, or commit decompiled or recovered proprietary source code.
- Do not copy or redistribute proprietary icons, logos, product names, screenshots used as brand assets, sounds, fonts, or other branding.
- Do not commit installers, executables, memory dumps, unpacked application resources, REA captures, or other proprietary binaries.
- Do not reproduce private algorithms, internal identifiers, hidden strings, or implementation structure learned through decompilation.
- Do not present inference as confirmed behavior.
- Do not use REA to bypass licensing, access controls, authentication, encryption, or technical protection measures.

NewPic implementation must be based only on approved requirements, public documentation, and independently written code. When evidence is uncertain or its use is questionable, record it under **Unknowns** and stop before implementation until the scope is reviewed.
