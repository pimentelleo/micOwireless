# Troubleshooting

## Desktop does not appear in mobile discovery

- Ensure desktop receiver is running.
- Verify both devices are on the same network.
- Open firewall for UDP ports: `<audio_port>` and `<audio_port + 1>`.

## Audio has delay or glitch

- Keep both devices on 5 GHz Wi-Fi.
- Reduce competing network traffic.
- Ensure desktop output device is stable and not switching automatically.

## Encrypted stream fails

- Confirm same pair code on both sides.
- Confirm secure mode toggle state matches on both apps.
- Restart stream session after changing pair code.

## Rust build fails with linker errors

- Install Visual Studio 2022 Build Tools with C++ workload.
- Reopen terminal after installation.
