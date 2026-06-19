# micOwireless

Open-source wireless microphone system inspired by WoMIC, with a glassmorphism UI and Material 3 design language.

## What it does

- Turns your phone into a wireless microphone for Windows.
- Streams PCM16 audio from mobile to desktop over UDP.
- Supports local-network desktop discovery.
- Supports pair-code encryption (ChaCha20-Poly1305).
- Includes jitter-tolerant playback and packet-loss concealment.

## Repository layout

- `mobile/` - Flutter app (audio capture and stream sender)
- `desktop/` - Tauri app (Rust receiver + React UI)

## Protocol

- Protocol ID: `mow2`
- Transport: UDP
- Discovery: UDP broadcast on `audio_port + 1`
- Audio payload: PCM16 mono (default 48 kHz)
- Optional encryption: ChaCha20-Poly1305 with pair-code-derived key

## Requirements

### Windows desktop

1. Node.js 20+
2. Rust (MSVC target)
3. Visual Studio 2022 Build Tools with C++ workload
4. (Optional but recommended) VB-Cable or another virtual audio cable

### Mobile

1. Flutter SDK (recommended via Puro)
2. Android Studio/Xcode as needed for target platform

## Setup

### Desktop

```powershell
cd desktop
npm install
npm run tauri dev
```

### Mobile (using Puro)

```powershell
cd mobile
puro -e stable flutter pub get
puro -e stable flutter run
```

For release APK generation in this repository, `flutter-dev.bat` was used with
configured `JAVA_HOME`, `ANDROID_HOME`, and `ANDROID_SDK_ROOT`.

## End-to-end usage

1. Open **Desktop app**.
2. Configure:
   - UDP port (default `49000`)
   - Output device (prefer your virtual cable input device)
   - Secure mode + pair code
3. Click **Start Receiver**.
4. Open **Mobile app**.
5. Tap **Discover** to find desktop automatically, or fill IP/port manually.
6. Use the same pair code (when secure mode is enabled).
7. Tap **Start Streaming**.
8. In Discord/OBS/Meet, select the virtual cable output as microphone input.

## Development checks

### Mobile

```powershell
cd mobile
puro -e stable flutter analyze
puro -e stable flutter test
```

### Desktop frontend

```powershell
cd desktop
npm run build
```

### Desktop Rust backend

```powershell
cd desktop\src-tauri
cargo check
```

### Rust protocol/discovery tests

```powershell
cd desktop\src-tauri
cargo test
```

## Release artifacts generated

- Desktop MSI: `desktop/src-tauri/target/release/bundle/msi/micOwireless Desktop_0.1.0_x64_en-US.msi`
- Desktop setup EXE: `desktop/src-tauri/target/release/bundle/nsis/micOwireless Desktop_0.1.0_x64-setup.exe`
- Android APK: `mobile/build/app/outputs/flutter-apk/app-release.apk`

## GitHub Releases automation

Version tags in the format `v*` trigger `.github/workflows/release.yml`, which builds:

- Windows installers (`.msi` + `.exe`)
- Android release APK

Then publishes all assets directly to the GitHub Release for that tag.

## Troubleshooting

- If mobile discovery returns no desktop:
  - Ensure desktop receiver is running.
  - Ensure both devices are on the same Wi-Fi/LAN.
  - Ensure local firewall allows UDP on chosen port and `port + 1`.
- If decryption errors occur:
  - Pair codes must match exactly on both apps.
  - Secure mode must be either enabled on both sides or disabled on both sides.
- If audio crackles:
  - Keep devices on 5 GHz Wi-Fi when possible.
  - Use lower network congestion channels.
  - Keep CPU-intensive background apps closed.

## Validation references

- `docs/E2E_VALIDATION_MATRIX.md`
- `docs/TROUBLESHOOTING.md`
- `docs/RELEASE.md`

## Open-source governance

- `LICENSE`
- `CODE_OF_CONDUCT.md`
- `CONTRIBUTING.md`
- `SECURITY.md`
- `.github/ISSUE_TEMPLATE/*`
- `.github/pull_request_template.md`
