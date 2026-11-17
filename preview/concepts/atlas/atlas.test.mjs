import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const here = new URL('.', import.meta.url);
const context = vm.createContext({ TextEncoder, Intl });
vm.runInContext(readFileSync(new URL('../../report.js', here), 'utf8'), context);
vm.runInContext(readFileSync(new URL('demo.js', here), 'utf8'), context);
const R = context.PruneReport;
const demo = () => JSON.parse(JSON.stringify(context.AtlasDemo));

test('embedded demo exactly matches the shared fictional fixture', () => {
  assert.deepEqual(demo(), JSON.parse(readFileSync(new URL('../../demo-report.json', here), 'utf8')));
  assert.equal(R.validateReport(demo()).total_bytes, 17490247680);
});

test('filters, sort and category proportions use report findings only', () => {
  const report = R.validateReport(demo());
  assert.equal(R.filterFindings(report.findings, { category: 'developer' }).length, 2);
  assert.equal(R.filterFindings(report.findings, { risk: 'high' })[0].rule_id, 'demo-system');
  assert.equal(R.filterFindings(report.findings, { query: 'SampleBrowser' })[0].category, 'browser');
  assert.equal(R.filterFindings(report.findings, { sort: 'size' })[0].size_bytes, 7516192768);
  assert.equal(report.findings.reduce((sum, f) => sum + f.size_bytes, 0), report.total_bytes);
});

test('cancelled, partial and suppressed error safety metadata survives validation', () => {
  const input = demo();
  input.cancelled = true; input.suppressed_errors = 3;
  input.findings[0].partial = true;
  const report = R.validateReport(input);
  assert.equal(report.partial, true);
  assert.equal(report.cancelled, true);
  assert.equal(report.suppressed_errors, 3);
  assert.equal(report.findings[0].partial, true);
});

test('mismatched totals rejected and unknown HTML fields discarded', () => {
  const input = demo();
  input.markup = '<img src=x onerror=alert(1)>';
  input.findings[0].html = '<script>alert(1)</script>';
  const report = R.validateReport(input);
  assert.equal(report.markup, undefined);
  assert.equal(report.findings[0].html, undefined);
  input.total_bytes++;
  assert.throws(() => R.validateReport(input), /итоги/);
});

test('concept keeps restrictive CSP and no markup sink', () => {
  const html = readFileSync(new URL('index.html', here), 'utf8');
  const app = readFileSync(new URL('app.js', here), 'utf8');
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /default-src 'none'/);
  assert.match(html, /src="\.\.\/\.\.\/report\.js"/);
  assert.doesNotMatch(app, /innerHTML|outerHTML|insertAdjacentHTML|fetch\(/);
});
