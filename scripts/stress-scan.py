#!/usr/bin/env python3
"""Read-only CLI smoke test against disposable fixtures, never real caches."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--candidates", type=int, default=2000)
    parser.add_argument("--temp-root", type=Path)
    args = parser.parse_args()
    if not 1 <= args.candidates <= 100_000:
        parser.error("--candidates must be between 1 and 100000")
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(
        prefix="prune-stress-", dir=args.temp_root
    ) as temporary:
        root = Path(temporary)
        home = root / "home"
        cache = home / "app" / "cache"
        rules = root / "rules"
        cache.mkdir(parents=True)
        rules.mkdir()
        for index in range(args.candidates):
            (cache / f"entry-{index:06d}").write_bytes(b"1234")
        (rules / "fixture.toml").write_text(
            'id = "stress-fixture"\n'
            'name = "Stress fixture"\n'
            'description = "Disposable files, not real user caches"\n'
            'os = ["macos", "linux", "windows"]\n'
            'category = "developer"\nrisk = "safe"\n'
            'recreate = "automatic"\npaths = ["~/app/cache/*"]\n',
            encoding="utf-8",
        )
        env = dict(os.environ, HOME=str(home), USERPROFILE=str(home))
        for workers in (1, 4):
            result = subprocess.run(
                [
                    str(binary), "scan", "--json", "--workers", str(workers),
                    "--rules-dir", str(rules),
                ],
                env=env, capture_output=True, timeout=180, check=True,
            )
            if result.stderr:
                raise AssertionError("JSON scan unexpectedly wrote to stderr")
            report = json.loads(result.stdout)
            assert report["partial"] is False, "scan unexpectedly partial"
            assert report["cancelled"] is False, "scan unexpectedly cancelled"
            assert not report["errors"], "scan reported errors"
            assert len(report["findings"]) == args.candidates
            assert report["total_files"] == args.candidates
            assert report["total_bytes"] == args.candidates * 4
            assert all(path.read_bytes() == b"1234" for path in cache.iterdir())
            print(f"PASS: {args.candidates} fixture candidates, {workers} workers")


if __name__ == "__main__":
    main()
