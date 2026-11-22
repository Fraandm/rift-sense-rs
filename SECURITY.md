# Security policy

## Supported versions

Security fixes are provided for the latest published release. Older installers
may be removed from GitHub Releases.

## Reporting a vulnerability

Do not open a public issue with exploit details. Contact the maintainer through
the repository's private security advisory feature:

<https://github.com/Fraandm/rift-sense-rs/security/advisories/new>

Include the version, Windows version, reproduction steps and whether the issue
affects the local API, installer, updater or League Client integration.

## Local API model

- The HTTP server binds to `127.0.0.1` by default.
- Requests with a non-local `Host` header are rejected.
- Browser requests with a foreign `Origin` are rejected.
- Static responses include a restrictive Content Security Policy and
  `X-Frame-Options: DENY`.
- External tools without an `Origin` header can still call the API by design;
  do not expose the server to untrusted networks.

## Out of scope

- Riot Games account bans, anti-cheat behavior or changes to the League Client.
- Vulnerabilities in Riot's client/API or Microsoft WebView2.
- Unsupported modified builds.
