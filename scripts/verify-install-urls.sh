#!/usr/bin/env bash
# Resolves every URL the docs advertise as a way to get tack, and fails if any
# of them doesn't come back with a 2xx. This is the only thing in the repo
# that would have caught the install one-liner pointing at a branch that
# didn't exist — nothing else touches these files together.
#
# Resolving a URL is not installing from it: a release can publish two
# archives that share the same platform suffix, the GitHub releases API can
# list the wrong one first, and install.sh's asset picker can grab it —
# every advertised URL still returns 2xx while the install itself fails.
# So after the URL sweep, this script actually runs the repository's
# install.sh against whatever release is really published, into a scratch
# directory, and asserts a genuine, runnable `tack` binary lands.
#
# Run from the repository root: scripts/verify-install-urls.sh
#
# If a new page starts advertising an install command, add its path to FILES.
set -euo pipefail

FILES=(
  README.md
  docs/DEPLOYMENT-GUIDE.md
  docs/book/src/user-guide/quick-start.md
  docs/book/src/roadmap.md
  install.sh
)

# The one-line installer's raw-content URL (whichever branch it names), and
# the releases page linked as the alternative "download a release" method.
pattern='https://(raw\.githubusercontent\.com/yielab/tack/[^[:space:]")]+|github\.com/yielab/tack/releases)'

mapfile -t urls < <(grep -hoE "$pattern" "${FILES[@]}" | sort -u)

if [ "${#urls[@]}" -eq 0 ]; then
  echo "verify-install-urls: found no install URLs in: ${FILES[*]}" >&2
  echo "verify-install-urls: the file list above is probably stale" >&2
  exit 1
fi

echo "Checking ${#urls[@]} install URL(s):"
printf '  %s\n' "${urls[@]}"
echo

fail=0
for url in "${urls[@]}"; do
  code="$(curl -s -o /dev/null -w '%{http_code}' -L --max-time 15 "$url" || echo 000)"
  if [ "$code" -ge 200 ] && [ "$code" -lt 300 ]; then
    printf 'OK   [%s] %s\n' "$code" "$url"
  else
    printf 'FAIL [%s] %s\n' "$code" "$url"
    fail=1
  fi
done

# ── Run the real installer, for real ────────────────────────────────────────
# Every URL above can resolve while the install itself still fails: a release
# can publish two archives sharing the "-${platform}.tar.gz" suffix, and if
# the releases API lists the one install.sh doesn't want first, its asset
# picker takes it. Only actually running install.sh, against the release
# that's really published right now, and checking what it leaves behind,
# catches that class of bug.
echo
echo "Running install.sh for real (not just resolving its URL):"
install_dir="$(mktemp -d)"
trap 'rm -rf "$install_dir"' EXIT

if TACK_INSTALL_DIR="$install_dir" sh install.sh; then
  if [ -x "$install_dir/tack" ] && version="$("$install_dir/tack" --version 2>&1)"; then
    printf 'OK   install.sh installed a working tack: %s\n' "$version"
  else
    printf 'FAIL install.sh exited 0 but left no working tack binary at %s/tack\n' "$install_dir" >&2
    fail=1
  fi
else
  printf 'FAIL install.sh exited non-zero — the advertised install command is broken\n' >&2
  fail=1
fi

exit "$fail"
