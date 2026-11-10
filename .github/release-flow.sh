#!/usr/bin/env bash
# The release flow diagram, for a workflow run's summary: the first ```mermaid block of
# docs/releasing.md (its one source, also what GitHub shows on that page), with the nodes named as
# arguments marked as what this run does. GitHub draws one run's jobs, not how promote.yml,
# release.yml, site.yml and the checks they read follow each other: this shows it on every run.
#
#   .github/release-flow.sh NODE... >>"$GITHUB_STEP_SUMMARY"
#
# A node id the diagram doesn't have is left out with a warning; a missing diagram prints nothing
# (a warning), never a failure: the summary is a report, not a check.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
doc="$root/docs/releasing.md"

body="$(awk '/^```mermaid$/ { p = 1; next } p && /^```$/ { exit } p' "$doc" 2>/dev/null || true)"
if [ -z "$body" ]; then
  echo "::warning::no mermaid diagram in docs/releasing.md: the release flow isn't shown" >&2
  exit 0
fi

marked=()
for node in "$@"; do
  # A node is declared as `id["label"]` at the start of a line.
  if grep -Eq "^[[:space:]]*${node}\[" <<<"$body"; then
    marked+=("$node")
  else
    echo "::warning::the release flow diagram has no node \"$node\"" >&2
  fi
done

echo "#### Release flow"
echo
echo "What this run does is marked; the arrows are the order (docs/releasing.md)."
echo
echo '```mermaid'
printf '%s\n' "$body"
if [ "${#marked[@]}" -gt 0 ]; then
  echo "  classDef here fill:#fde68a,stroke:#b45309,stroke-width:3px,color:#111111"
  (IFS=,; echo "  class ${marked[*]} here")
fi
echo '```'
