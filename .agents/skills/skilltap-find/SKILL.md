---
name: skilltap-find
description: Discover and search for agent skills via configured taps and the npm registry.
license: MIT
---
# skilltap-find

Search for installable agent skills via taps or npm. Use `--json` for machine-readable output.

## Search taps

```bash
skilltap find                     # list all skills in configured taps
skilltap find <query>             # filter by name, description, or tags
skilltap find <query> --json
```

Output fields (JSON): `tap`, `name`, `description`, `trust`

## Search npm

```bash
skilltap find --npm               # list all agent-skill packages on npm
skilltap find --npm <query>
skilltap find --npm <query> --json
```

## Trust tiers

- `provenance` — SLSA attestation verified via Sigstore
- `publisher` — known npm publisher
- `curated` — tap maintainer manually verified
- `unverified` — no verification; review before installing

## Install a found skill

Once identified, install by tap name (no URL required):

```bash
skilltap install <skill-name>
skilltap install <skill-name> --project
skilltap install <skill-name>@v1.2.0
```

## Manage taps

Taps are git repos containing a `tap.json` registry. Register them once; then search and install by name.

```bash
skilltap tap add <name> <url>     # clone and register
skilltap tap remove <name>
skilltap tap list
skilltap tap update               # pull latest for all taps
skilltap tap update <name>
```

Add the official tap:
```bash
skilltap tap add skilltap https://github.com/nklisch/skilltap-skills
```
