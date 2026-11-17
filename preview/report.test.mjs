import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { validateReport, parseReport, displayText, formatBytes, filterFindings, MAX_BYTES } from './report.mjs';
const demo = JSON.parse(readFileSync(new URL('./demo-report.json', import.meta.url), 'utf8'));
const copy = () => structuredClone(demo);
test('demo follows schema 1 and exact totals', () => {
  assert.equal(validateReport(demo).findings.length, 6);
  assert.equal(parseReport(JSON.stringify(demo)).total_bytes, demo.total_bytes);
});
test('reject invalid JSON, version and oversized files', () => {
  assert.throws(() => parseReport('{'));
  assert.throws(() => parseReport(' '.repeat(MAX_BYTES + 1)));
  const r = copy(); r.schema_version = 2; assert.throws(() => validateReport(r));
});
test('reject hostile schema types and unknown enum values', () => {
  for (const [key, value] of [['path', {}], ['path', null], ['risk', 'constructor'], ['risk', 'unknown'], ['category', '__proto__'], ['recreate', 'other'], ['description', []], ['rule_name', 'x'.repeat(8193)], ['last_modified', -1], ['size_bytes', Infinity], ['file_count', 1.5], ['partial', 'false']]) {
    const r = copy(); r.findings[0][key] = value; assert.throws(() => validateReport(r), key);
  }
});
test('reject negative, unsafe, inconsistent totals, accounting and array bounds', () => {
  for (const [key, value] of [['total_bytes', -1], ['total_files', Number.MAX_SAFE_INTEGER + 1], ['total_bytes', 0], ['byte_accounting', 'allocated'], ['errors', Array(1001).fill({})], ['findings', Array(10001).fill({})]]) {
    const r = copy(); r[key] = value; assert.throws(() => validateReport(r), key);
  }
});
test('conservatively marks partial reports including cancelled and suppressed errors', () => {
  for (const key of ['cancelled', 'partial', 'errors', 'suppressed_errors', 'finding']) {
    const r = copy();
    if (key === 'errors') r.errors = [{ path: '/example/denied', message: 'Access denied' }];
    else if (key === 'finding') r.findings[0].partial = true;
    else r[key] = key === 'suppressed_errors' ? 4 : true;
    assert.equal(validateReport(r).partial, true);
  }
});
test('old schema 1 optional flags default to logical and false', () => {
  const r = copy();
  for (const key of ['partial', 'cancelled', 'suppressed_errors', 'byte_accounting']) delete r[key];
  assert.equal(validateReport(r).partial, false);
});
test('hostile markup remains literal text; backslash, controls and bidi are escaped', () => {
  const html = '<img src=x onerror=alert(1)>';
  const r = copy(); r.findings[0].path = html;
  assert.equal(displayText(validateReport(r).findings[0].path), html);
  assert.equal(displayText('a\\b\n\u202e\u2066\u0000'), 'a\\\\b\\u000a\\u202e\\u2066\\u0000');
  const app = readFileSync(new URL('./app.js', import.meta.url), 'utf8');
  assert.doesNotMatch(app, /innerHTML|outerHTML|insertAdjacentHTML|eval\(|fetch\(/);
});
test('filter, search and sorting are deterministic and do not mutate input', () => {
  const findings = validateReport(demo).findings;
  assert.equal(filterFindings(findings, { category: 'developer' }).length, 2);
  assert.equal(filterFindings(findings, { risk: 'high' })[0].category, 'system');
  assert.equal(filterFindings(findings, { query: 'SAMPLEBROWSER' })[0].category, 'browser');
  assert.equal(filterFindings(findings, { sort: 'risk' })[0].risk, 'high');
  assert.equal(filterFindings(findings, { sort: 'size' })[0].size_bytes, 7516192768);
  assert.equal(findings[0].index, undefined);
});
test('binary sizes are honest and support zero', () => {
  assert.equal(formatBytes(0), '0 Б');
  assert.equal(formatBytes(1024), '1 КиБ');
  assert.equal(formatBytes(1024 ** 3), '1 ГиБ');
});
test('Windows paths are searchable both as pasted paths and escaped display text', () => {
  const findings = [{ ...validateReport(demo).findings[0], path: String.raw`C:\Users\Alice\AppData\Local\Temp` }];
  assert.equal(filterFindings(findings, { query: findings[0].path }).length, 1);
  assert.equal(filterFindings(findings, { query: displayText(findings[0].path) }).length, 1);
  assert.equal(filterFindings(findings, { query: String.raw`c:\users\alice` }).length, 1);
});
