import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { categories, risks, pathEncodings, lossyPathWarning, validateReport, parseReport, displayText, formatBytes, sumIntegers, compareIntegers, ratioBasisPoints, percentage, dateFromSeconds, scopeDetails, filterFindings, MAX_BYTES } from './report.mjs';
const demo = JSON.parse(readFileSync(new URL('./demo-report.json', import.meta.url), 'utf8'));
const copy = () => structuredClone(demo);
const digest = 'sha256:' + 'a'.repeat(64);
const scope = () => ({
  root_kind: 'home', comparable: false, min_size_bytes: 4096,
  categories: ['browser', 'developer'], max_risk: 'high', rule_set_digest: digest
});
const v2 = (completeness = 'complete') => {
  const r = copy();
  r.schema_version = 2;
  delete r.partial;
  delete r.cancelled;
  r.completeness = completeness;
  r.scope = scope();
  return r;
};
test('demo follows schema 1 and exact totals', () => {
  const normalized = validateReport(demo);
  assert.equal(normalized.findings.length, 6);
  assert.equal(normalized.scope, undefined);
  assert.equal(typeof normalized.findings[0].size_bytes, 'number');
  assert.equal(parseReport(JSON.stringify(demo)).total_bytes, demo.total_bytes);
});
test('reject invalid JSON, version and oversized files', () => {
  assert.throws(() => parseReport('{'));
  assert.throws(() => parseReport(' '.repeat(MAX_BYTES + 1)));
  const r = copy(); r.schema_version = 3; assert.throws(() => validateReport(r), /schema_version/);
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
test('rejects diagnostic aggregates that exceed safe integer precision', () => {
  const overflow = copy();
  overflow.errors = [{ path: '/example/denied', message: 'Access denied' }];
  overflow.suppressed_errors = Number.MAX_SAFE_INTEGER;
  assert.throws(() => validateReport(overflow), /сумма errors и suppressed_errors/);
  const maximumSafe = copy();
  maximumSafe.suppressed_errors = Number.MAX_SAFE_INTEGER;
  assert.equal(validateReport(maximumSafe).diagnostic_count, Number.MAX_SAFE_INTEGER);
  const aggregateBoundary = copy();
  aggregateBoundary.errors = [{ path: '/example/denied', message: 'Access denied' }];
  aggregateBoundary.suppressed_errors = Number.MAX_SAFE_INTEGER - 1;
  assert.equal(validateReport(aggregateBoundary).diagnostic_count, Number.MAX_SAFE_INTEGER);
});
test('schema 1 completeness preserves legacy signals and treats absent partial as unknown', () => {
  const cases = [
    [r => { delete r.partial; }, 'unknown'],
    [r => { r.partial = false; }, 'complete'],
    [r => { r.partial = true; }, 'partial'],
    [r => { delete r.partial; r.errors = [{ path: '/example/denied', message: 'Access denied' }]; }, 'partial'],
    [r => { delete r.partial; r.suppressed_errors = 1; }, 'partial'],
    [r => { delete r.partial; r.findings[0].partial = true; }, 'partial'],
    [r => { r.partial = false; r.errors = [{ path: '/example/denied', message: 'Access denied' }]; }, 'partial'],
    [r => { r.partial = false; r.suppressed_errors = 1; }, 'partial'],
    [r => { r.partial = false; r.findings[0].partial = true; }, 'partial'],
    [r => { r.cancelled = true; }, 'cancelled'],
    [r => { r.cancelled = true; delete r.partial; r.errors = [{ path: '/example/denied', message: 'Access denied' }]; }, 'cancelled'],
    [r => { r.cancelled = true; r.partial = false; }, 'cancelled'],
    [r => { r.cancelled = true; r.partial = true; }, 'cancelled']
  ];
  for (const [configure, completeness] of cases) {
    const r = copy(); configure(r);
    const report = validateReport(r);
    assert.equal(report.completeness, completeness);
    assert.equal(report.partial, completeness !== 'complete');
  }
});
test('path encoding defaults to UTF-8 and accepts only the v2 lossy marker', () => {
  assert.ok(Object.keys(pathEncodings).includes('utf8'));
  assert.ok(Object.keys(pathEncodings).includes('lossy_utf8'));
  assert.ok(lossyPathWarning.includes('не используйте эту строку как точный путь'));
  assert.ok(validateReport(demo).findings.every(f => f.path_encoding === 'utf8'));
  const lossy = v2();
  lossy.findings[0].path_encoding = 'lossy_utf8';
  assert.equal(validateReport(lossy).findings[0].path_encoding, 'lossy_utf8');
  for (const value of ['lossy', '__proto__', null, {}]) {
    const invalid = v2();
    invalid.findings[0].path_encoding = value;
    assert.throws(() => validateReport(invalid), /path_encoding/);
  }
});
test('v1 omitted optional fields remain valid while completeness stays conservative', () => {
  const r = copy();
  for (const key of ['partial', 'cancelled', 'suppressed_errors', 'byte_accounting']) delete r[key];
  const normalized = validateReport(r);
  assert.equal(normalized.completeness, 'unknown');
  assert.equal(normalized.partial, true);
  assert.equal(normalized.byte_accounting, 'logical');
  const stringNumber = copy();
  stringNumber.total_bytes = String(stringNumber.total_bytes);
  assert.throws(() => validateReport(stringNumber), /total_bytes/);
});
test('schema 2 normalizes all four explicit completeness states', () => {
  const cases = ['complete', 'partial', 'cancelled', 'unknown'];
  for (const completeness of cases) {
    const r = v2(completeness);
    r.partial = completeness === 'partial';
    r.cancelled = completeness === 'cancelled';
    const normalized = validateReport(r);
    assert.equal(normalized.schema_version, 2);
    assert.equal(normalized.completeness, completeness);
    assert.equal(normalized.partial, undefined);
    assert.equal(normalized.cancelled, undefined);
    assert.equal(typeof normalized.total_bytes, 'bigint');
    assert.deepEqual(normalized.scope, { ...scope(), min_size_bytes: 4096n });
  }
  const cancelled = validateReport(v2('cancelled'));
  assert.equal(cancelled.completeness, 'cancelled');
  assert.equal(cancelled.cancelled, undefined);
  const unknownWithLegacyFalse = v2('unknown');
  unknownWithLegacyFalse.partial = false;
  const normalizedUnknown = validateReport(unknownWithLegacyFalse);
  assert.equal(normalizedUnknown.completeness, 'unknown');
  assert.equal(normalizedUnknown.partial, undefined);
});
test('schema 2 rejects malformed scope values and digests', () => {
  const cases = [
    ['scope', null],
    ['root_kind', '__proto__'],
    ['root_kind', 'constructor'],
    ['root_kind', 'path'],
    ['comparable', 1],
    ['min_size_bytes', -1],
    ['min_size_bytes', 1.5],
    ['min_size_bytes', Number.MAX_SAFE_INTEGER + 1],
    ['categories', 'developer'],
    ['categories', [null]],
    ['categories', ['']],
    ['categories', Array(1)],
    ['categories', Array(129).fill('developer')],
    ['categories', ['custom-category']],
    ['categories', ['__proto__']],
    ['categories', ['constructor']],
    ['categories', ['developer', 'developer']],
    ['categories', ['<img src=x onerror=alert(1)>']],
    ['max_risk', {}],
    ['max_risk', ''],
    ['max_risk', 'unknown'],
    ['max_risk', '__proto__'],
    ['max_risk', 'constructor'],
    ['rule_set_digest', 'sha256:' + 'A'.repeat(64)],
    ['rule_set_digest', 'sha256:' + 'a'.repeat(63)],
    ['rule_set_digest', 'sha512:' + 'a'.repeat(64)]
  ];
  for (const [field, value] of cases) {
    const r = v2();
    if (field === 'scope') r.scope = value;
    else r.scope[field] = value;
    assert.throws(() => validateReport(r), field);
  }
  const noScope = v2(); delete noScope.scope;
  assert.throws(() => validateReport(noScope), /scope/);
  const missing = v2(); delete missing.scope.rule_set_digest;
  assert.throws(() => validateReport(missing), /scope\.rule_set_digest/);
  const badCompleteness = v2(); badCompleteness.completeness = '__proto__';
  assert.throws(() => validateReport(badCompleteness), /completeness/);
});
test('schema 2 rejects contradictory completeness, legacy flags and diagnostic evidence', () => {
  for (const completeness of ['complete', 'partial', 'cancelled', 'unknown']) {
    for (const partial of [false, true]) {
      for (const cancelled of [false, true]) {
        const r = v2(completeness);
        r.partial = partial;
        r.cancelled = cancelled;
        const consistent = partial === (completeness === 'partial') && cancelled === (completeness === 'cancelled');
        if (consistent) assert.doesNotThrow(() => validateReport(r));
        else assert.throws(() => validateReport(r), /completeness/);
      }
    }
  }
  const cases = [
    r => { r.cancelled = true; },
    r => { r.partial = true; },
    r => { r.completeness = 'partial'; r.cancelled = true; },
    r => { r.completeness = 'partial'; r.partial = false; },
    r => { r.completeness = 'cancelled'; r.cancelled = false; },
    r => { r.completeness = 'cancelled'; r.cancelled = true; r.partial = true; },
    r => { r.completeness = 'unknown'; r.cancelled = true; },
    r => { r.completeness = 'unknown'; r.partial = true; },
    r => { r.errors = [{ path: '/example/denied', message: 'Access denied' }]; },
    r => { r.suppressed_errors = 1; },
    r => { r.findings[0].partial = true; }
  ];
  for (const configure of cases) {
    const r = v2(); configure(r);
    assert.throws(() => validateReport(r), /completeness/);
  }
});
test('schema 2 unknown remains distinct and cannot contradict known incomplete evidence', () => {
  const unknownWithFalseFlags = v2('unknown');
  unknownWithFalseFlags.partial = false;
  unknownWithFalseFlags.cancelled = false;
  assert.equal(validateReport(unknownWithFalseFlags).completeness, 'unknown');
  const withErrors = v2('unknown');
  withErrors.errors = [{ path: '/example/denied', message: 'Access denied' }];
  assert.throws(() => validateReport(withErrors), /completeness/);
  const withPartialFinding = v2('unknown');
  withPartialFinding.findings[0].partial = true;
  assert.throws(() => validateReport(withPartialFinding), /completeness/);
});
test('schema 2 cancellation remains the state when diagnostics and partial findings exist', () => {
  const r = v2('cancelled');
  r.cancelled = true;
  r.partial = false;
  r.errors = [{ path: '/example/denied', message: 'Access denied' }];
  r.suppressed_errors = 2;
  r.findings[0].partial = true;
  assert.equal(validateReport(r).completeness, 'cancelled');
});
test('schema 2 parses u64 strings exactly and uses exact sums, sorting and ratios', () => {
  const base = copy().findings[0];
  const r = v2('complete');
  r.scanned_at = '18446744073709551615';
  r.findings = [
    { ...base, rule_id: 'large-first', rule_name: 'Large first', path: '/example/first',
      size_bytes: '9007199254740992', file_count: '9007199254740992', last_modified: '9007199254740993' },
    { ...base, rule_id: 'large-second', rule_name: 'Large second', path: '/example/second',
      size_bytes: '9007199254740993', file_count: '9007199254740993', last_modified: '18446744073709551615' }
  ];
  r.total_bytes = '18014398509481985';
  r.total_files = '18014398509481985';
  r.suppressed_errors = '0';
  r.scope.min_size_bytes = '9007199254740993';
  const normalized = parseReport(JSON.stringify(r));
  assert.equal(normalized.scanned_at, 18446744073709551615n);
  assert.equal(normalized.total_bytes, 18014398509481985n);
  assert.equal(normalized.total_files, 18014398509481985n);
  assert.equal(normalized.findings[0].size_bytes, 9007199254740992n);
  assert.equal(normalized.findings[1].file_count, 9007199254740993n);
  assert.equal(normalized.findings[1].last_modified, 18446744073709551615n);
  assert.equal(normalized.findings[0].path_encoding, 'utf8');
  assert.equal(normalized.scope.min_size_bytes, 9007199254740993n);
  assert.equal(normalized.diagnostic_count, 0n);
  assert.deepEqual(validateReport(normalized), normalized);
  assert.deepEqual(filterFindings(normalized.findings, { sort: 'size' }).map(f => f.rule_id), ['large-second', 'large-first']);
  assert.equal(filterFindings(normalized.findings, { query: 'second' }).length, 1);
  assert.equal(sumIntegers(normalized.findings.map(f => f.size_bytes), 0n), normalized.total_bytes);
  assert.equal(compareIntegers(normalized.findings[1].size_bytes, normalized.findings[0].size_bytes), 1);
  assert.equal(ratioBasisPoints(normalized.findings[0].size_bytes, normalized.total_bytes), 5000);
  assert.equal(percentage(normalized.findings[0].size_bytes, normalized.total_bytes), 50);
});
test('schema 2 accepts zero counts and rejects noncanonical or out-of-range u64 values', () => {
  const base = copy().findings[0];
  const zero = v2('complete');
  zero.scanned_at = '0';
  zero.findings = [{ ...base, size_bytes: '0', file_count: '0', last_modified: null }];
  zero.total_bytes = '0'; zero.total_files = '0'; zero.suppressed_errors = '0'; zero.scope.min_size_bytes = '0';
  const normalized = validateReport(zero);
  assert.equal(normalized.total_bytes, 0n);
  assert.equal(normalized.total_files, 0n);
  assert.equal(normalized.findings[0].file_count, 0n);
  assert.equal(formatBytes(normalized.total_bytes), '0 Б');
  assert.equal(ratioBasisPoints(0n, 0n), 0);
  assert.equal(dateFromSeconds(0n).getTime(), 0);
  assert.equal(dateFromSeconds(18446744073709551615n), null);

  const mutations = [
    r => { r.scanned_at = '01'; },
    r => { r.total_bytes = '-1'; },
    r => { r.total_files = '1e3'; },
    r => { r.findings[0].size_bytes = '18446744073709551616'; },
    r => { r.findings[0].file_count = ''; },
    r => { r.findings[0].last_modified = '00'; },
    r => { r.suppressed_errors = '18446744073709551616'; },
    r => { r.scope.min_size_bytes = '18446744073709551616'; },
    r => { r.scope.min_size_bytes = Number.MAX_SAFE_INTEGER + 1; }
  ];
  for (const mutate of mutations) {
    const invalid = v2();
    mutate(invalid);
    assert.throws(() => validateReport(invalid));
  }
});
test('schema 2 scope keeps only safe fields and renders hostile strings as text', () => {
  const r = v2();
  r.scope.root_path = '/Users/private/secret';
  r.scope.extra = '<img src=x onerror=alert(1)>';
  const normalized = validateReport(r);
  const details = scopeDetails(normalized.scope);
  assert.equal(normalized.scope.root_path, undefined);
  assert.doesNotMatch(JSON.stringify(details), /\/Users\/private\/secret/);
  assert.doesNotMatch(JSON.stringify(details), /<img src=x/);
  assert.equal(details[3][0], 'Максимальный риск (включительно)');
  assert.equal(details[3][1], risks.high);
  assert.equal(scopeDetails({ ...normalized.scope, max_risk: null })[3][1], 'Не задан');
});
test('scope category and risk filters use the actual enums', () => {
  const r = v2();
  r.scope.categories = Object.keys(categories).sort();
  r.scope.max_risk = null;
  assert.deepEqual(validateReport(r).scope.categories, Object.keys(categories).sort());
  for (const risk of Object.keys(risks)) {
    const input = v2();
    input.scope.max_risk = risk;
    assert.equal(validateReport(input).scope.max_risk, risk);
  }
  const unsorted = v2();
  unsorted.scope.categories = ['system', 'developer'];
  assert.throws(() => validateReport(unsorted), /scope.categories/);
});
test('public scope describes root kind but never claims pairwise comparability', () => {
  for (const rootKind of ['home', 'custom', 'empty']) {
    const r = v2();
    r.scope.root_kind = rootKind;
    r.scope.comparable = false;
    const normalized = validateReport(r);
    assert.equal(normalized.scope.root_kind, rootKind);
    assert.equal(normalized.scope.comparable, false);
    assert.equal(scopeDetails(normalized.scope)[0][1], rootKind === 'home' ? 'Домашняя папка' :
      rootKind === 'custom' ? 'Пользовательская область (путь не указан)' : 'Пустой набор');
    assert.equal(scopeDetails(normalized.scope).some(([label]) => /сопоставим/i.test(label)), false);
  }
  for (const rootKind of ['home', 'custom', 'empty']) {
    const r = v2();
    r.scope.root_kind = rootKind;
    r.scope.comparable = true;
    assert.throws(() => validateReport(r), /scope\.comparable/);
  }
  const custom = v2();
  custom.scope.root_kind = 'custom';
  custom.scope.comparable = false;
  custom.scope.root_path = '/private/identity';
  const normalizedCustom = validateReport(custom);
  assert.equal(normalizedCustom.scope.root_path, undefined);
  assert.match(scopeDetails(normalizedCustom.scope)[0][1], /путь не указан/);
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
  assert.equal(formatBytes(0n), '0 Б');
  assert.equal(formatBytes(1536n), '1,5 КиБ');
  assert.equal(formatBytes(9007199254740993n), '8 ПиБ');
  assert.equal(formatBytes(1024 ** 3), '1 ГиБ');
});
test('Windows paths are searchable both as pasted paths and escaped display text', () => {
  const findings = [{ ...validateReport(demo).findings[0], path: String.raw`C:\Users\Alice\AppData\Local\Temp` }];
  assert.equal(filterFindings(findings, { query: findings[0].path }).length, 1);
  assert.equal(filterFindings(findings, { query: displayText(findings[0].path) }).length, 1);
  assert.equal(filterFindings(findings, { query: String.raw`c:\users\alice` }).length, 1);
});
