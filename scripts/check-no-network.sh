#!/usr/bin/env bash
set -euo pipefail

# Heuristic only: known dependency names and source API spellings, not a sandbox
# or proof. Aliases, macros, FFI, subprocesses and dependency internals can evade it.
cd "$(dirname "${BASH_SOURCE[0]}")/.."
export LC_ALL=C
deny='^(reqwest|hyper|hyper-util|ureq|isahc|curl|curl-sys|surf|attohttpc|tiny_http|tokio-tungstenite|tungstenite|quinn|h2|rustls|native-tls|openssl|socket2|async-net)( v|$)'
apis='(std[[:space:]]*::[[:space:]]*net|TcpStream|TcpListener|UdpSocket|reqwest|hyper[[:space:]]*::|ureq[[:space:]]*::|isahc[[:space:]]*::|tokio[[:space:]]*::[[:space:]]*net|async_std[[:space:]]*::[[:space:]]*net|socket2[[:space:]]*::)'

for package in prune-core prune-cli; do
  # Capture separately: a failed cargo must never be interpreted as "no matches".
  # Inspect all target-specific edges and features, including dev/build deps.
  if tree=$(cargo tree --locked -p "$package" --all-features --target all --prefix none); then
    :
  else
    status=$?
    echo "error: cargo tree failed for $package (status $status)" >&2
    exit "$status"
  fi
  # Do not use grep -q: it can hide read errors after an early match.
  if matches=$(grep -E "$deny" <<< "$tree"); then
    echo "error: known network-capable dependency found in $package" >&2
    printf '%s\n' "$matches" >&2
    exit 1
  else
    status=$?
    if [[ "$status" -ne 1 ]]; then
      echo "error: dependency grep failed for $package (status $status)" >&2
      exit "$status"
    fi
  fi

  if grep -RInE "$apis" "crates/$package/src"; then
    echo "error: known network API referenced in $package" >&2
    exit 1
  else
    status=$?
    if [[ "$status" -ne 1 ]]; then
      echo "error: source grep failed for $package (status $status)" >&2
      exit "$status"
    fi
  fi
done

echo "ok: no known network dependencies/API patterns in core + CLI (heuristic only)"
