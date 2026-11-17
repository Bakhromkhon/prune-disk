#!/usr/bin/env bash
set -euo pipefail

# Isolated fixtures only: never modify the real workspace or invoke real cargo.
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/prune-network-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/scripts" "$tmp/bin" "$tmp/crates/prune-core/src" "$tmp/crates/prune-cli/src"
cp "$root/scripts/check-no-network.sh" "$tmp/scripts/"
real_grep=$(command -v grep)
export REAL_GREP="$real_grep"
cat > "$tmp/bin/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" -eq 9 && "$1" == tree && "$2" == --locked && "$3" == -p &&
   "$5" == --all-features && "$6" == --target && "$7" == all &&
   "$8" == --prefix && "$9" == none ]] || exit 99
printf '%s\n' "$4" >> "$CALL_LOG"
printf '%s v0.1.0\n' "$4"
if [[ "$4" == "$TEST_PACKAGE" ]]; then
  printf '%s\n' "$TEST_DEP"
  exit "$CARGO_STATUS"
fi
SH
cat > "$tmp/bin/grep" <<'SH'
#!/usr/bin/env bash
if [[ "$GREP_FAILURE" == dependency && "$1" == -E ]] ||
   [[ "$GREP_FAILURE" == source && "$1" == -RInE ]]; then
  echo 'simulated grep read error' >&2
  exit 2
fi
exec "$REAL_GREP" "$@"
SH
chmod +x "$tmp/bin/cargo" "$tmp/bin/grep"
export PATH="$tmp/bin:$PATH"
export CALL_LOG="$tmp/calls" TEST_PACKAGE=prune-core TEST_DEP='' CARGO_STATUS=0 GREP_FAILURE=''
count=0

run_case() {
  local expected=$1 label=$2 status=0
  : > "$CALL_LOG"
  bash "$tmp/scripts/check-no-network.sh" > "$tmp/output" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    echo "FAIL: $label (expected $expected, got $status)" >&2
    cat "$tmp/output" >&2
    exit 1
  fi
  if [[ "$status" -ne 0 ]] && "$REAL_GREP" -q '^ok:' "$tmp/output"; then
    echo "FAIL: $label emitted success" >&2
    exit 1
  fi
  count=$((count + 1))
}

printf 'fn main() {}\n' > "$tmp/crates/prune-core/src/lib.rs"
printf 'fn main() {}\n' > "$tmp/crates/prune-cli/src/main.rs"
run_case 0 'clean source and dependency graphs'
[[ $(cat "$CALL_LOG") == $'prune-core\nprune-cli' ]]
"$REAL_GREP" -q 'heuristic only' "$tmp/output"

for TEST_PACKAGE in prune-core prune-cli; do
  export TEST_PACKAGE
  CARGO_STATUS=42
  run_case 42 "$TEST_PACKAGE cargo failure with partial clean output"
  TEST_DEP='reqwest v0.12.0'
  run_case 42 "$TEST_PACKAGE cargo failure with partial forbidden output"
  CARGO_STATUS=0
  for dep in reqwest hyper hyper-util ureq isahc curl curl-sys surf attohttpc tiny_http \
    tokio-tungstenite tungstenite quinn h2 rustls native-tls openssl socket2 async-net; do
    TEST_DEP="$dep v1.0.0"
    run_case 1 "$TEST_PACKAGE forbidden dependency $dep"
  done
  TEST_DEP='not-reqwest v1.0.0'
  run_case 0 "$TEST_PACKAGE dependency name boundary"
  TEST_DEP=''
  for api in 'std::net' 'std :: net' TcpStream TcpListener UdpSocket reqwest \
    'hyper::' 'ureq::' 'isahc::' 'tokio::net' 'async_std::net' 'socket2::'; do
    printf '%s\n' "$api" > "$tmp/crates/$TEST_PACKAGE/src/probe.rs"
    run_case 1 "$TEST_PACKAGE forbidden API $api"
  done
  rm "$tmp/crates/$TEST_PACKAGE/src/probe.rs"
done

for GREP_FAILURE in dependency source; do
  export GREP_FAILURE
  run_case 2 "$GREP_FAILURE grep error"
done
GREP_FAILURE=''
mv "$tmp/crates/prune-cli/src" "$tmp/crates/prune-cli/missing-src"
run_case 2 'missing CLI source directory'
printf 'ok: %s network heuristic regression cases\n' "$count"
