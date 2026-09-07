# Security Policy

## Scope

Tack is a **self-hosted, local-first** tool designed to run on `127.0.0.1`. It has
no built-in authentication beyond the optional `TACK_API_TOKEN` bearer token — one
shared token, no per-user identities — and is not intended to be exposed to the
public internet without an authenticating reverse proxy.

## Reporting a Vulnerability

**Please do not open a public issue for security vulnerabilities.**

Report privately through **GitHub Security Advisories**, which keeps the report
confidential until a fix is released:

1. Go to <https://github.com/yielab/tack/security/advisories/new>
   (repository **Security** tab → **Report a vulnerability**).
2. Describe the issue, affected versions, and reproduction steps.

If you cannot use GitHub Security Advisories, email
**[info@yielab.com](mailto:info@yielab.com)** with the subject `[tack] security`.

Please include, where possible:

- A description of the vulnerability and its impact.
- Steps to reproduce, or a proof-of-concept.
- The affected version / commit and your configuration (bind address, whether
  `TACK_API_TOKEN` is set, reverse proxy, etc.).

### What to expect

- **Acknowledgement** within 7 days.
- An assessment and, for confirmed issues, a remediation plan with a target
  timeline shared with you.
- A coordinated disclosure: a fix is released before public details, and you are
  credited in the advisory and `CHANGELOG.md` unless you prefer to remain
  anonymous.

Please give us a reasonable window to release a fix before any public disclosure.

## Supported Versions

Only the latest release is actively maintained. Fixes land on `develop` and ship in
the next release; older tags are not backported.

| Version | Supported |
| --- | --- |
| Latest release | ✅ |
| Older releases | ❌ |

## Known Design Constraints

These are intentional properties of the current single-user, local-first design —
not vulnerabilities — but worth understanding when deploying:

| Area | Notes |
| --- | --- |
| Authentication | Optional shared bearer token (`TACK_API_TOKEN`). Not a replacement for network-level controls; no per-user accounts. |
| Exposure | Binds `127.0.0.1` by default. Binding a non-loopback address without `TACK_API_TOKEN` is refused at startup; overriding that refusal with `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` exposes an unauthenticated read/write API and a full-database download endpoint. |
| SQLite | Single-file DB, no encryption at rest. |
| File uploads | Max 50 MB. Stored in `TACK_STORAGE_DIR` with UUID filenames; not executed or served as active content. |
| CORS | Configurable via `TACK_ALLOWED_ORIGINS`; defaults to localhost only. |

For hardening guidance, see the
[Security checklist](docs/DEPLOYMENT-GUIDE.md#security-checklist) in the deployment
guide.
