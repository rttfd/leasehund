# Contributing to leasehund

## Git Workflow

### Protected Main

- **`main` is protected** — never push directly to `main`.
- All changes must go through a pull request.
- PRs require CI to pass before merge.

### Branch Naming

All branches must follow the pattern:

```
<type>/<short-description>
```

**Allowed types:**
- `feat` — new features
- `fix` — bug fixes
- `chore` — maintenance, dependencies, tooling
- `docs` — documentation only
- `refactor` — code restructuring without functional change
- `test` — adding or updating tests
- `release` — version bumps and release prep

**Examples:**
- `feat/run-with-callback`
- `fix/lease-expiry`
- `chore/bump-smoltcp`

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

<body>
```

**Types:** `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `ci`

### CI Requirements

All PRs must pass `make ci` (formatting, `check` and `lint` on every feature set, the tests, and
the documentation build) and branch naming validation. See `AGENTS.md` for the code rules.

### Releasing

1. Update `CHANGELOG.md` under `[Unreleased]`
2. Bump version in `Cargo.toml`
3. Create a `release/vX.Y.Z` branch
4. Open PR, merge to `main`
5. Tag `main` with `vX.Y.Z`
6. Push tag (triggers release workflow)
