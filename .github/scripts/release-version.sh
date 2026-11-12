#!/usr/bin/env bash
# Checks a release tag against the workspace version (add-web-build-and-releases,
# design decision 13): the tag must be `v` followed by `[workspace.package]
# version` of Cargo.toml, exactly. Prints `version=<V>` and
# `prerelease=<true|false>` (a version with a `-` suffix, such as
# 0.1.0-rc.1, is a pre-release) and appends both to $GITHUB_OUTPUT when set.
#
# Usage: .github/scripts/release-version.sh <tag> [Cargo.toml]
set -euo pipefail

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <tag> [Cargo.toml]" >&2
  exit 2
fi
tag=$1
manifest=${2:-Cargo.toml}

# The `version = "..."` line of the [workspace.package] table.
version=$(awk '
  /^\[/ { in_table = ($0 == "[workspace.package]") ; next }
  in_table && /^version[[:space:]]*=/ {
    sub(/^version[[:space:]]*=[[:space:]]*"/, ""); sub(/".*$/, ""); print; exit
  }
' "$manifest")
if [ -z "$version" ]; then
  echo "error: $manifest has no [workspace.package] version" >&2
  exit 1
fi
if [ "$tag" != "v$version" ]; then
  echo "error: tag $tag does not match the workspace version $version (expected v$version)" >&2
  exit 1
fi

case $version in
  *-*) prerelease=true ;;
  *) prerelease=false ;;
esac

echo "version=$version"
echo "prerelease=$prerelease"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "version=$version"
    echo "prerelease=$prerelease"
  } >> "$GITHUB_OUTPUT"
fi
