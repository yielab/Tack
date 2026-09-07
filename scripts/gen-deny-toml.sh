#!/usr/bin/env bash
# Generates deny.toml (main workspace) and crates/tack-desktop/deny.toml
# (the desktop workspace, its own Cargo.lock) — the single source of truth
# for `cargo deny`'s policy in each, used identically by `make deny` and the
# CI `deny` job so the two can never drift. Neither file is committed (see
# CLAUDE.md: generated artifacts stay out of version control); both are
# regenerated every run instead.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Shared by both workspaces: license allowlist and ban rules. The two
# workspaces never disagree on which licenses or duplicate-version policy
# they accept, only on which advisories they've already reviewed.
common_policy() {
  cat <<'EOF'
[graph]
all-features = true

[bans]
# Duplicate (multiple-version) dependencies are reported as an
# advisory (warning), not a hard failure.
multiple-versions = "warn"
wildcards = "allow"

[licenses]
version = 2
confidence-threshold = 0.8
allow = [
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "MPL-2.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "Unlicense",
  "0BSD",
  "BSL-1.0",
  "OpenSSL",
  "CDLA-Permissive-2.0",
]
EOF
}

common_policy > deny.toml

# crates/tack-desktop pulls in Tauri's Linux WebView backend (wry), which
# still depends on the unmaintained gtk-rs GTK3 bindings and, through
# urlpattern 0.3.0, the unmaintained unic-* Unicode crates. Every entry below
# is reviewed individually rather than exempted as a block; each cites the
# crate, why it's unavoidable at the current Tauri 2.x release, and when to
# check again.
cat > crates/tack-desktop/deny.toml <<'EOF'
[advisories]
ignore = [
  # gtk-rs GTK3 bindings: wry pins gtk = "0.18" and webkit2gtk = "=2.0"
  # through its newest published release (0.56.1), so no Tauri 2.x version
  # moves off GTK3. Re-review when wry publishes a GTK4 backend.
  { id = "RUSTSEC-2024-0413", reason = "atk 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0416", reason = "atk-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0412", reason = "gdk 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0418", reason = "gdk-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0411", reason = "gdkwayland-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0417", reason = "gdkx11 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0414", reason = "gdkx11-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0415", reason = "gtk 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0420", reason = "gtk-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0419", reason = "gtk3-macros 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  # Same GTK3 binding line as above, pulled in via glib-macros 0.18.5 rather
  # than directly by wry.
  { id = "RUSTSEC-2024-0370", reason = "proc-macro-error 1.0.4, pulled in by glib-macros 0.18.5 (the gtk-rs GTK3 line); not an independent dependency choice. Reviewed 2026-09-07, next review 2027-03-07." },
  # unic-* Unicode crates: pulled in by urlpattern 0.3.0, which tauri-utils
  # 2.9.3 (Tauri's newest tauri-utils release) pins with `version = "0.3"`.
  # urlpattern 0.6.0 already replaced these with icu_properties; re-review
  # once tauri-utils raises its urlpattern requirement.
  { id = "RUSTSEC-2025-0081", reason = "unic-char-property 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0075", reason = "unic-char-range 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0080", reason = "unic-common 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0100", reason = "unic-ucd-ident 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0098", reason = "unic-ucd-version 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
]

EOF
common_policy >> crates/tack-desktop/deny.toml
