# E2E Validation Matrix

| Scenario | Method | Result |
| --- | --- | --- |
| Rust backend protocol parsing (plain/encrypted) | `cargo test` (`parse_plain_packet`, `parse_encrypted_packet`) | ✅ Pass |
| Discovery responder behavior | `cargo test` (`discovery_responder_replies`) | ✅ Pass |
| Jitter/loss handling logic | `cargo test` (`jitter_buffer_drops_missing_packets`) | ✅ Pass |
| Desktop production build | `npm run tauri build` | ✅ Pass |
| Android production build | `flutter-dev.bat build apk --release` | ✅ Pass |
| Mobile static checks and widget smoke test | `flutter analyze`, `flutter test` | ✅ Pass |
| Physical phone -> Windows Wi-Fi session | Manual device run | ⏳ Pending operator run |

## Manual physical run checklist

1. Install desktop release bundle (`.msi` or `.exe`).
2. Install mobile `app-release.apk`.
3. Set same pair code in both apps with secure mode enabled.
4. Start desktop receiver.
5. Discover desktop from mobile and start streaming.
6. Route desktop output through virtual cable.
7. Validate audio input in target apps (Discord/OBS/Meet).
8. Record observed latency, jitter, and reconnection behavior.
