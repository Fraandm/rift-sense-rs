# Privacy policy

RiftSense is a local desktop application. It does not create an account, does
not upload your match data, and does not include analytics or telemetry.

## What is stored locally

- `%APPDATA%\RiftSense\settings.json`: interface, startup and tray preferences.
- `%LOCALAPPDATA%\RiftSense\logs`: rotating diagnostic logs, kept for up to
  14 days and capped by size.
- `%LOCALAPPDATA%\Programs\RiftSense\data\client_location.json`: optional
  League of Legends installation path.
- The installed application and bundled models/data.

## What is read

- League Client `lockfile` and local LCU endpoints: connection state, champion
  select, summoner name, level and gameflow phase.
- Live Client Data API: champion, KDA, CS, level, gold, items, team and map
  information during a match.
- No passwords, tokens from the lockfile, or private messages are included in
  logs or diagnostics.

## Network access

- RiftSense checks GitHub Releases for updates and downloads the installer and
  its SHA-256 checksum when you request an update.
- The interface loads fonts from Google Fonts and champion/item icons from
  Riot's Data Dragon CDN.
- The local HTTP API listens on `127.0.0.1` by default and rejects non-local
  hosts and cross-origin requests.

## Diagnostics

The "Copy diagnostics" action includes the version, patch, dataset dates, model
status, local paths, process ID and local client status. Review the JSON before
sharing it publicly; it is designed not to contain credentials, but local paths
and summoner name may be present.

## Contact

Open an issue at <https://github.com/Fraandm/rift-sense-rs> for privacy
questions or deletion requests.
