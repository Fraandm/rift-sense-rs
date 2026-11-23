# Release validation

This checklist is the minimum for a public 1.5.0 build. Automated checks are in
`.github/workflows/ci.yml`; installer checks are in
`scripts/validate-installer.ps1`.

## Automated

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo build --bins

.\scripts\validate-installer.ps1 `
  -Installer .\dist\RiftSense-Setup-1.5.0.exe `
  -ExpectedSha256 "4c41b71b21cca871af44506e01cf47085d4233e5bc4305ea67c07541fc89ca29" `
  -AllowExistingInstall
```

The 1.5.0 build is unsigned because no Authenticode certificate is configured;
add `-RequireSignature` once a signed build is available. The installer validator
checks SHA-256, Authenticode, silent install, version,
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
| ARAM/Mayhem/Arena draft | Mode detected from LCU; no roles/side/bans; composition A1–A5 / R1–R5; Arena trio. |
| Post-game summary | Result, real build, purchases, recommendation follow-rate, KDA and manual result fallback. |
| Arena/Mayhem manual | Trio, up to 4 augments, anvils for Arena; data persisted in the summary. |
| Summoner's Rift, live | KDA/CS/levels/gold/items update every 3 s, next purchase and component. |
| Full inventory | Components do not end the build; pinned next legendary, reuse cost, 2 alternatives and "Completar" consumes components. |
| ARAM confidence | With model, 5 enemies, live context and sample, confidence is Alta; a complete build shows Build completa instead of a low percentage. |
| Legendary target | After buying a component of the pinned legendary, the target stays; it changes only on completion or champion/role/mode change. |
| Own pick | Hover/locked pick is detected, shown as a ghost in Draft, excluded from recommendations and counted in the pick total. |
| Enemy roles | Enemy roles are jointly estimated from champion-role priors, marked with ≈; explicit client roles, when present, are respected. |
| Boots | Boots card recommends AP/AD/tank/support boots, reacts to enemy autoattack/AP builds, shows owned boots or the free tier-3 upgrade, and respects a full inventory. |
| Enemy builds | Each enemy row shows items, KDA, level, damage type and threat; consumables, trinkets, jungle pets and unknown IDs are filtered out. |
| Enemy build analysis | Enemy item builds produce armor/MR/HP/heal/shield analysis, per-enemy focus notes, adapted penetration/antiheal/defense reasons and situational items from different categories. |
| Bot role quest | 6 complete items + boots in the dedicated slot; 7 inventory holes; no extra boots recommended. |
| Support item | World Atlas -> Runic Compass -> Bounty of Worlds -> final upgrade; support item occupies one normal slot. |
| Mid boots | Tier 2 boots offered as a free tier 3 replacement after the role quest; no extra slot consumed. |
| Jungle / Doran | Jungle pets only for jungle; Doran not offered to support with a support item. |
| Overlay | With the setting enabled, a compact transparent window appears over League when it is foreground; it shows the next purchase, gold, boots and one situational item, hides on alt-tab and toggles with Ctrl+Shift+O and the tray item. |
| Overlay input | The overlay is click-through and never takes focus; no game input, purchase or automation is sent. |
| Overlay display | Works in borderless/windowed fullscreen; if the game uses exclusive fullscreen, the app documents that Windows may not compose normal windows above it. |
| Responsive UI | 390/768/1440 px without horizontal scroll; sticky mobile buy bar; inventory remove buttons visible on touch. |
| UI feedback | No CSP/console errors, toasts auto-dismiss and clear on tab change, accessible names and focus visible. |
| ARAM | Map 12/35/36, no illegal items, no-recall advice, CS as reference. |
| ARAM: Mayhem (`KIWI`) | Mode label, exclusive items, item legality, augment limitation notice. |
| Arena (`CHERRY`) | Whole-item prices, no components, CS/roles hidden, subteam fallback, anvil notes. |
| Client closed | Manual mode, manual data, no crash, clear error state. |
| Reconnect | LCU phase changes, no duplicate window/tray entry, webview recovers. |
| Game end | Live data stops, no stale recommendation, app remains usable. |
| Fullscreen / borderless | App can be restored from tray and remains readable on a second monitor. |
| Windows startup | Starts minimized, one instance, tray icon, settings persisted. |
| Update | 1.4.0 -> 1.5.0, checksum, relaunch, settings preserved. |
| Uninstall | Binaries removed, startup entry removed, user settings/log policy clear. |

## Acceptance criteria

- No P0 bug open; crash-free sessions during the matrix.
- No illegal or duplicate item recommended in 1,000 generated builds.
- Update and rollback tested from the previous public version.
- Signed installer (or an explicit documented exception).
- Diagnostics contain no lockfile password or secret.
- README, privacy, license, third-party notices and security policy published.
- Modes with limited data are visibly marked and explain their limitations.
