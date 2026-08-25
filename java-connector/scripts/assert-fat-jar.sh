#!/usr/bin/env bash
set -euo pipefail
JAR="${1:?usage: assert-fat-jar.sh <jar-file>}"
MIN_BYTES="${FAT_JAR_MIN_BYTES:-1000000}"

if [[ ! -f "$JAR" ]]; then
  echo "missing jar: $JAR" >&2
  exit 1
fi

size=$(wc -c < "$JAR" | tr -d ' ')
echo "jar size: $size bytes ($JAR)"
if (( size < MIN_BYTES )); then
  echo "jar is too small to be a multi-platform fat jar (min $MIN_BYTES)" >&2
  jar tf "$JAR" | head >&2 || true
  exit 1
fi

required=(
  ogsql_linux_amd64
  ogsql_linux_arm64
  ogsql_osx_amd64
  ogsql_osx_arm64
  ogsql_windows_amd64.exe
)
listing=$(jar tf "$JAR")
missing=0
for res in "${required[@]}"; do
  if ! grep -qxF "$res" <<<"$listing"; then
    echo "missing resource: $res" >&2
    missing=1
  fi
done
if (( missing )); then
  echo "jar contents:" >&2
  echo "$listing" >&2
  exit 1
fi
echo "fat-jar check passed"
