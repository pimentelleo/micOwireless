# SignPath Foundation readiness checklist

This checklist maps the repository to the practical requirements for applying to SignPath Foundation.

## Compliance matrix

| Requirement | Status | Evidence |
| --- | --- | --- |
| OSI-approved open-source license | ✅ | `LICENSE` (MIT) |
| Public source repository | ✅ | Public GitHub repository with releases |
| Project is maintained and released | ✅ | GitHub Releases `v0.0.x` and active CI workflow |
| Project functionality documented | ✅ | `README.md`, `docs/RELEASE.md`, `docs/TROUBLESHOOTING.md` |
| Code signing policy published | ✅ | `CODE_SIGNING_POLICY.md` and links in `README.md`/`docs/RELEASE.md` |
| Privacy policy published | ✅ | `PRIVACY_POLICY.md` and links in `README.md`/`docs/RELEASE.md` |
| Team roles defined (Committers/Reviewers/Approvers) | ✅ | `CODE_SIGNING_POLICY.md` |
| Uninstallation guidance exists | ✅ | `README.md` uninstall section |
| Release artifacts from verifiable CI build | ✅ | `.github/workflows/release.yml` |
| Product/version metadata aligned for release artifacts | ✅ | `desktop/package.json`, `desktop/src-tauri/Cargo.toml`, `desktop/src-tauri/tauri.conf.json`, `mobile/pubspec.yaml` |

## Operational prerequisites before applying

1. Keep MFA enabled for maintainers and signing approvers.
2. Keep signing approval restricted to designated approvers.
3. Apply at <https://signpath.org/apply.html> after reviewing <https://signpath.org/terms.html>.

