# Release validation

This checklist is the minimum for a public 1.0 build. Automated checks are in
`.github/workflows/ci.yml`; installer checks are in
`scripts/validate-installer.ps1`.

## Automated

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo build --bins

.\scripts\validate-installer.ps1 `
  -Installer .\dist\RiftSense-Setup-1.0.0.exe `
  -ExpectedSha256 "<hash>" `
  -RequireSignature
```

The installer validator checks SHA-256, Authenticode, silent install, version,
local health endpoint and uninstall. Run it in a clean VM or Windows Sandbox:
the installer uses the same `AppId` as the real application, so validating on a
machine with a real installation can remove its Start menu shortcut during the
temporary uninstall. The script now refuses to run in that case unless
`-AllowExistingInstall` is explicitly passed.

## Manual matrix

Run at least 10 matches per mode, on Windows 10 or 11, with a clean install:

| Scenario | What to verify |
| --- | --- |
| Summoner's Rift, draft | Roles, bans, side, import from LCU, next build. |
| Summoner's Rift, live | KDA/CS/levels/gold/items update every 3 s, next purchase and component. |
| ARAM | Map 12/35/36, no illegal items, no-recall advice, CS as reference. |
| ARAM: Mayhem (`KIWI`) | Mode label, exclusive items, item legality, augment limitation notice. |
| Arena (`CHERRY`) | Whole-item prices, no components, CS/roles hidden, subteam fallback, anvil notes. |
| Client closed | Manual mode, manual data, no crash, clear error state. |
| Reconnect | LCU phase changes, no duplicate window/tray entry, webview recovers. |
| Game end | Live data stops, no stale recommendation, app remains usable. |
| Fullscreen / borderless | App can be restored from tray and remains readable on a second monitor. |
| Windows startup | Starts minimized, one instance, tray icon, settings persisted. |
| Update | 0.5.x → 1.0.0, checksum, relaunch, settings preserved. |
| Uninstall | Binaries removed, startup entry removed, user settings/log policy clear. |

## Acceptance criteria

- No P0 bug open; crash-free sessions during the matrix.
- No illegal or duplicate item recommended in 1,000 generated builds.
- Update and rollback tested from the previous public version.
- Signed installer (or an explicit documented exception).
- Diagnostics contain no lockfile password or secret.
- README, privacy, license, third-party notices and security policy published.
- Modes with limited data are visibly marked and explain their limitations.
