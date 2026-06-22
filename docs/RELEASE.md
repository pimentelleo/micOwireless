# Release Guide

## Automated GitHub release (recommended)

Pushing to `main` triggers `.github/workflows/release.yml` to:

1. Calculate next SemVer from commit messages.
2. Build Windows installers (`.msi` and NSIS `.exe`) and a portable Windows ZIP (`.zip`).
3. Build Android release APK.
4. Create tag `vX.Y.Z`.
5. Publish all binaries directly to the matching GitHub Release.

Windows bundles are generated with `webviewInstallMode = skip`, so the installer does not bundle WebView2. It expects the runtime to already exist on the target system.

SemVer rules:

- `BREAKING CHANGE` or `type(scope)!:` -> major
- `feat:` -> minor
- anything else -> patch

If a commit message includes `[skip release]`, the workflow skips creating a release for that push.

### Windows code-signing (Defender/SmartScreen)

To reduce Windows Defender/SmartScreen install blocking, configure Authenticode signing in repository secrets:

1. `WINDOWS_CODESIGN_CERT_PFX_BASE64` - Base64 content of your `.pfx` certificate file.
2. `WINDOWS_CODESIGN_CERT_PASSWORD` - Password for the `.pfx`.
3. Optional repository variable: `WINDOWS_CODESIGN_TIMESTAMP_URL` (defaults to `http://timestamp.digicert.com`).

Notes:

- Prefer an EV code-signing certificate for faster SmartScreen reputation.
- Without these secrets, workflow still publishes Windows artifacts, but they remain unsigned and are more likely to be blocked/warned.

## Code signing policy

- [Code signing policy](../CODE_SIGNING_POLICY.md)
- [Privacy policy](../PRIVACY_POLICY.md)
- [SignPath readiness checklist](SIGNPATH_READINESS.md)

## Desktop release

```powershell
cd desktop
npm install
npm run tauri build
```

Generated artifacts are in:

- `desktop/src-tauri/target/release/bundle/`
- `desktop/src-tauri/target/release/bundle/portable/` (portable ZIP)

## Mobile release (Android example)

```powershell
cd mobile
puro -e stable flutter build apk --release
```

Generated artifact:

- `mobile/build/app/outputs/flutter-apk/app-release.apk`

## Release checklist

1. Run all validation commands from README.
2. Validate real-device desktop/mobile end-to-end stream.
3. Confirm discovery and encrypted pairing behavior.
4. Publish release notes with known limitations.
