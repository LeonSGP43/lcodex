#!/usr/bin/env bash
set -euo pipefail

branch="${1:-main}"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Error: run this script inside a git repository."
  exit 1
fi

if ! git remote get-url upstream >/dev/null 2>&1; then
  echo "Error: upstream remote is not configured."
  echo "Add it with:"
  echo "  git remote add upstream https://github.com/openai/codex.git"
  exit 1
fi

echo "Fetching upstream..."
git fetch upstream

echo "Checking out ${branch}..."
git checkout "${branch}"

echo "Fast-forward merge from upstream/${branch}..."
git merge --ff-only "upstream/${branch}"

echo "Done. ${branch} is now synced with upstream/${branch}."
