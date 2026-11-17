#!/usr/bin/env bash
set -euo pipefail

deny='^(reqwest|hyper|ureq|isahc|curl|surf|attohttpc|tiny_http|tokio-tungstenite|tungstenite|quinn|h2|rustls|native-tls|openssl)( v|$)'

if cargo tree -p prune-core --prefix none | grep -Eq "$deny"; then
  echo "error: network-capable dependency found in prune-core" >&2
  cargo tree -p prune-core --prefix none | grep -E "$deny" >&2
  exit 1
fi

if grep -RInE '(std::net|TcpStream|TcpListener|UdpSocket|reqwest|hyper::|ureq::|isahc::)' crates/prune-core/src; then
  echo "error: network API referenced in prune-core" >&2
  exit 1
fi

echo "ok: prune-core is network-free"
