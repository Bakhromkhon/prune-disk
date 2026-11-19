#!/usr/bin/env node
// End-to-end check: real Rust v2 CLI JSON must be accepted by the offline viewer.
// All files are created inside a unique disposable home subdirectory.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, relative, sep } from 'node:path';
import { parseReport } from '../preview/report.mjs';

const binaryArg = process.argv[2] ?? `target/release/prune${process.platform === 'win32' ? '.exe' : ''}`;
const binary = binaryArg.startsWith(sep) || /^[A-Za-z]:[\\/]/.test(binaryArg)
  ? binaryArg
  : join(process.cwd(), binaryArg);
const home = homedir();
assert.ok(home && home.length > 1, 'home directory must be available');
const fixtureRoot = await mkdtemp(join(home, '.prune-report-contract-'));

function run(args) {
  const result = spawnSync(binary, args, { encoding: 'utf8', maxBuffer: 5 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, '', 'JSON scan must not emit progress or warnings');
  return result.stdout;
}

function assertNoHomeStrings(value, homes) {
  if (typeof value === 'string') {
    for (const prefix of homes) {
      const insensitive = process.platform === 'win32' || process.platform === 'darwin';
      assert.equal(
        insensitive ? value.toLocaleLowerCase('en-US').includes(prefix.toLocaleLowerCase('en-US')) : value.includes(prefix),
        false,
        'redacted report still contains a home-directory string'
      );
    }
  } else if (Array.isArray(value)) {
    value.forEach(item => assertNoHomeStrings(item, homes));
  } else if (value && typeof value === 'object') {
    Object.values(value).forEach(item => assertNoHomeStrings(item, homes));
  }
}

try {
  const cache = join(fixtureRoot, 'cache');
  const rules = join(fixtureRoot, 'rules');
  await mkdir(cache);
  await mkdir(rules);
  await writeFile(join(cache, 'sample.bin'), Buffer.from('12345'));

  const relativeRoot = relative(home, fixtureRoot).split(sep).join('/');
  assert.ok(relativeRoot && !relativeRoot.startsWith('../'), 'fixture must be below home');
  const os = { darwin: 'macos', win32: 'windows', linux: 'linux' }[process.platform];
  assert.ok(os, `unsupported test platform: ${process.platform}`);
  const description = JSON.stringify(`Temporary fixture under ${home}/private-note`);
  const rule = [
    'id = "contract-fixture"',
    'name = "Contract fixture"',
    `description = ${description}`,
    `os = ["${os}"]`,
    'category = "developer"',
    'risk = "safe"',
    'recreate = "automatic"',
    `paths = ["~/${relativeRoot}/cache/*"]`,
    '',
  ].join('\n');
  await writeFile(join(rules, 'fixture.toml'), rule);

  const common = ['scan', '--json', '--no-tui', '--workers', '1', '--rules-dir', rules];
  const rawText = run(common);
  const raw = parseReport(rawText);
  assert.equal(raw.schema_version, 2);
  assert.equal(raw.completeness, 'complete');
  assert.equal(raw.scope.root_kind, 'home');
  assert.equal(raw.scope.comparable, false);
  assert.equal(raw.findings.length, 1);
  assert.equal(raw.total_bytes, 5n);
  assert.ok(raw.findings[0].path.includes('sample.bin'));
  const canonicalHome = await realpath(home);
  assert.ok(
    raw.findings[0].path.includes(sep),
    'default JSON keeps a native absolute path for compatibility'
  );

  const redactedText = run([...common, '--redact-home']);
  const redacted = parseReport(redactedText);
  const redactedJson = JSON.parse(redactedText);
  assert.equal(redacted.schema_version, 2);
  assert.equal(redacted.findings[0].path.startsWith('~/') || redacted.findings[0].path.startsWith('~\\'), true);
  assertNoHomeStrings(redactedJson, [home, canonicalHome]);
  assert.deepEqual(await readFile(join(cache, 'sample.bin')), Buffer.from('12345'));

  console.log('PASS: Rust v2 JSON and redacted JSON both validate in the offline viewer');
} finally {
  await rm(fixtureRoot, { recursive: true, force: true });
}
