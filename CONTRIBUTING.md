# Contributing

Thanks for contributing to micOwireless.

## Development setup

### Desktop

```powershell
cd desktop
npm install
npm run build
cd src-tauri
cargo check
```

### Mobile

```powershell
cd mobile
puro -e stable flutter pub get
puro -e stable flutter analyze
puro -e stable flutter test
```

## Pull request expectations

1. Keep changes focused and well-scoped.
2. Document behavior changes in `README.md` when relevant.
3. Include validation commands in the PR description.
4. Add/update tests when behavior changes.

## Maintainer security requirements

1. Maintainers and release approvers must have MFA enabled on GitHub and on the signing platform.
2. Release signing approval must be performed only by designated approvers listed in `CODE_SIGNING_POLICY.md`.
3. Never sign binaries that are not produced from this repository's audited release workflow.

## Style guidance

- Prefer readability over cleverness.
- Avoid silent error swallowing.
- Keep networking and audio-path changes measurable and observable.
