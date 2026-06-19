# Release Guide

## Automated GitHub release (recommended)

Pushing to `main` triggers `.github/workflows/release.yml` to:

1. Calculate next SemVer from commit messages.
2. Build Windows installers (`.msi` and NSIS `.exe`).
3. Build Android release APK.
4. Create tag `vX.Y.Z`.
5. Publish all binaries directly to the matching GitHub Release.

Windows bundles are generated with `webviewInstallMode = offlineInstaller`, so the installer includes WebView2 runtime setup for end users.

SemVer rules:

- `BREAKING CHANGE` or `type(scope)!:` -> major
- `feat:` -> minor
- anything else -> patch

If a commit message includes `[skip release]`, the workflow skips creating a release for that push.

## Desktop release

```powershell
cd desktop
npm install
npm run tauri build
```

Generated artifacts are in:

- `desktop/src-tauri/target/release/bundle/`

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
