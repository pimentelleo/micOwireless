# micOwireless

Open-source wireless microphone system inspired by WoMIC, with glassmorphism UI and Material 3 design language.

## What this app does

- It is a single system that requires **both apps together**: Android (microphone sender) + Windows (audio receiver).
- Uses your Android phone microphone as a wireless mic for your Windows PC.
- Solves the "no mic / bad mic on PC" problem for calls and streaming tools.
- Routes audio to apps like Discord, OBS, and Meet (optionally via virtual audio cable).
- Streams over local Wi-Fi with desktop discovery and optional pair-code encryption.

## User quick start (Windows + Android together)

If you only want to use the app (not develop it), follow this section.  
Important: this is **not** two standalone apps — the Android app depends on the Windows app running, and vice versa.

### Download

Get the latest binaries from GitHub Releases:

- Release page: [https://github.com/pimentelleo/micOwireless/releases](https://github.com/pimentelleo/micOwireless/releases)
- Windows installer: `.msi` or `.exe`
- Windows portable: `.zip` (extract and run, no installation)
- Android app: `.apk`

### What to install on Windows

1. **micOwireless Desktop** from the latest release:
   - Installer mode: `.msi` or `.exe`
   - Portable mode: `.zip` (extract and run `micOwireless-desktop.exe`)
2. **Optional (recommended for Discord/OBS/Meet):** [VB-CABLE](https://vb-audio.com/Cable/) or another virtual audio cable.
3. **WebView2 Runtime** (usually already available on Windows 10/11).
4. Nothing else is required for end users (no Node.js, Rust, or Visual Studio tools).

Note: VB-CABLE is a third-party driver and remains an optional separate install.  
WebView2 is not bundled in the installer. If your environment does not have it, install it manually from Microsoft: [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

### Minimum user requirements

1. Windows 10 or 11 (x64).
2. Android phone (APK is currently provided for Android release flow).
3. Phone and PC on the same Wi-Fi/LAN.
4. Local firewall allowing UDP on the selected port (default `49000`) and `port + 1` for discovery.

## How to use

1. Open **micOwireless Desktop** on Windows (`.msi/.exe` install or portable `.zip` extraction).
2. In Desktop:
   - Keep port `49000` (or choose another one).
   - Choose output device:
     - `CABLE Input` (or equivalent) if you want to use it as a mic in other apps.
     - Speakers/headphones if you only want local monitoring.
   - Optional (experimental): tune **Receiver Tuning** (jitter startup/max pending and max output buffer).
   - Optional: enable **Secure mode** and define a pair code.
   - Click **Start Receiver**.
3. Install and open **micOwireless Mobile** on Android.
4. In Mobile:
   - Tap **Discover** (or enter desktop IP + port manually).
   - If secure mode is on, use the same pair code.
   - Optional (experimental): tune **Streaming Lab** (packet duration, mono/stereo, AGC/echo/noise processing).
   - Tap **Start Streaming**.
5. In Discord/OBS/Meet, select the virtual cable output as the microphone input.

### Uninstall

1. Windows (installer mode): open **Settings > Apps > Installed apps**, find **micOwireless Desktop**, and uninstall.
2. Windows (portable mode): delete the extracted portable folder.
3. Android: uninstall **micOwireless Mobile** from the system app settings.
4. Optional: uninstall VB-CABLE separately if you no longer need virtual audio routing.

## User troubleshooting

- No desktop found on mobile:
  - Check both devices are on the same network.
  - Confirm receiver is running on Windows.
  - Confirm firewall allows UDP on `49000` and `49001` (or your custom port pair).
- Secure stream not connecting:
  - Pair code must match exactly in both apps.
  - Secure mode must be enabled on both apps or disabled on both.
- Crackling or unstable audio:
  - Prefer 5 GHz Wi-Fi.
  - Reduce network congestion.
  - Close CPU-heavy background apps.
- Stream stops when Android screen turns off:
  - Keep the app updated (Android now uses a foreground streaming service).
  - Do not dismiss the persistent streaming notification while transmitting.
  - Set battery mode to unrestricted for micOwireless on aggressive OEM ROMs.

## For developers

### Repository layout

- `mobile/` - Flutter app (audio capture and stream sender)
- `desktop/` - Tauri app (Rust receiver + React UI)

### Protocol

- Protocol ID: `mow2`
- Transport: UDP
- Discovery: UDP broadcast on `audio_port + 1`
- Audio payload: PCM16 mono (default 48 kHz)
- Optional encryption: ChaCha20-Poly1305 with pair-code-derived key

### Development requirements

#### Windows desktop

1. [Node.js 20+](https://nodejs.org/en/download)
2. [Rust (rustup, MSVC target)](https://rustup.rs/)
3. [Visual Studio 2022 Build Tools (Desktop development with C++)](https://visualstudio.microsoft.com/visual-cpp-build-tools/)

#### Mobile

1. [Flutter SDK](https://docs.flutter.dev/get-started/install) (recommended via [Puro](https://puro.dev/))
2. [Android Studio](https://developer.android.com/studio) / [Xcode](https://developer.apple.com/xcode/) as needed for target platform

### Setup

#### Desktop

```powershell
cd desktop
npm install
npm run tauri dev
```

#### Mobile (using Puro)

```powershell
cd mobile
puro -e stable flutter pub get
puro -e stable flutter run
```

For release APK generation in this repository, `flutter-dev.bat` was used with configured `JAVA_HOME`, `ANDROID_HOME`, and `ANDROID_SDK_ROOT`.

### Development checks

#### Mobile

```powershell
cd mobile
puro -e stable flutter analyze
puro -e stable flutter test
```

#### Desktop frontend

```powershell
cd desktop
npm run build
```

#### Desktop Rust backend

```powershell
cd desktop\src-tauri
cargo check
```

#### Rust protocol/discovery tests

```powershell
cd desktop\src-tauri
cargo test
```

### Release automation

Every push to `main` triggers `.github/workflows/release.yml`, which:

1. Calculates the next SemVer automatically from commit messages.
2. Builds Windows installers (`.msi` + `.exe`), Windows portable ZIP (`.zip`), and Android release APK.
3. Creates the release tag (`vX.Y.Z`) in the same workflow.
4. Publishes all assets to the matching GitHub Release.

SemVer bump rules:

- `BREAKING CHANGE` or `type(scope)!:` -> major
- `feat:` -> minor
- anything else -> patch

For Windows trust/reputation, configure Authenticode signing secrets described in `docs/RELEASE.md` (`WINDOWS_CODESIGN_CERT_PFX_BASE64` and `WINDOWS_CODESIGN_CERT_PASSWORD`).

## Code signing policy

- [Code signing policy](CODE_SIGNING_POLICY.md)
- [Privacy policy](PRIVACY_POLICY.md)
- [SignPath readiness checklist](docs/SIGNPATH_READINESS.md)

## Validation references

- `docs/E2E_VALIDATION_MATRIX.md`
- `docs/TROUBLESHOOTING.md`
- `docs/RELEASE.md`

## Open-source governance

- `LICENSE`
- `CODE_OF_CONDUCT.md`
- `CONTRIBUTING.md`
- `CODE_SIGNING_POLICY.md`
- `PRIVACY_POLICY.md`
- `SECURITY.md`
- `.github/ISSUE_TEMPLATE/*`
- `.github/pull_request_template.md`
