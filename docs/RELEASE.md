# Release Guide

## Automated GitHub release (recommended)

Pushing a version tag (`v*`) triggers `.github/workflows/release.yml` to:

1. Build Windows installers (`.msi` and NSIS `.exe`).
2. Build Android release APK.
3. Publish all binaries directly to the matching GitHub Release.

```powershell
git tag v1.0.0
git push origin v1.0.0
```

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
