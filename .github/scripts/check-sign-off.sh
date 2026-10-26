#!/usr/bin/env bash
# Fails when a commit between BASE and HEAD (merges excluded) has no
# "Signed-off-by: Name <email>" line. Commits by Dependabot are exempt.
# usage: check-sign-off.sh BASE HEAD
set -euo pipefail
base=$1
head=$2
missing=0
checked=0
for commit in $(git rev-list --no-merges "$base..$head"); do
  author=$(git log -1 --format='%ae' "$commit")
  case "$author" in
    *'dependabot[bot]'*) continue ;;
  esac
  checked=$((checked + 1))
  if ! git log -1 --format='%B' "$commit" | grep -qE '^Signed-off-by: .+ <[^>]+@[^>]+>$'; then
    echo "::error::$(git log -1 --format='%h %s' "$commit") has no Signed-off-by line. Add it with 'git rebase --signoff main' (see CONTRIBUTING.md)."
    missing=$((missing + 1))
  fi
done
echo "Checked $checked commits; $missing without a sign-off."
[ "$missing" -eq 0 ]
