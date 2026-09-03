#!/usr/bin/env bash
# Guard for Constitution VII: `domain` depends on nothing internal and does no IO.
#
# The MemoryKernel trait lives in `domain` (a trait is exactly what belongs there, like SessionHost).
# The failure mode this guards against is subtle: someone adds `reqwest::Url` or `serde_json::Value`
# to a trait signature "just for now", `domain` grows a transport dependency, and the seam that was
# supposed to let a daemon replace the kernel quietly stops being a seam. A dependency diff catches
# that in review; a code review of a one-line signature change often does not.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXPECTED="$ROOT/scripts/domain-deps.expected"

actual="$(cargo tree -p terminal-ai-domain --depth 1 --prefix none 2>/dev/null | awk 'NF' | sort -u)"

if ! diff -u "$EXPECTED" <(echo "$actual"); then
  echo >&2
  echo "error: terminal-ai-domain's direct dependencies changed." >&2
  echo "If this is intentional, the new dependency must not be a transport or IO crate," >&2
  echo "and scripts/domain-deps.expected must be updated in the same commit." >&2
  exit 1
fi
echo "domain dependencies unchanged ($(echo "$actual" | wc -l | tr -d ' ') entries)"
