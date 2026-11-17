import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const context = vm.createContext({ TextEncoder, Intl });
for (const file of ['../../report.js', 'model.js', 'demo.js']) {
  vm.runInContext(fs.readFileSync(new URL(file, import.meta.url), 'utf8'), context);
}
const { PruneReport: R, FocusModel: M } = context;
const demo = R.validateReport(context.FocusDemo);

test('embedded fixture exactly matches root demo', () => {
  const fixture = JSON.parse(fs.readFileSync(new URL('../../demo-report.json', import.meta.url), 'utf8'));
  assert.equal(JSON.stringify(demo), JSON.stringify(R.validateReport(fixture)));
});
test('category, search and sort are real filters', () => {
  assert.equal(M.page(demo.findings, { category: 'developer' }).matches.length, 2);
  assert.equal(M.page(demo.findings, { query: 'SampleBrowser' }).items[0].category, 'browser');
  assert.equal(M.page(demo.findings, { sort: 'risk' }).items[0].risk, 'high');
  assert.equal(M.page(demo.findings, { query: 'no match' }, 99).items.length, 0);
});
test('pagination clamps and preserves original selection indexes', () => {
  const many = Array.from({ length: 19 }, (_, i) => ({ ...demo.findings[0], size_bytes: 100 - i }));
  const result = M.page(many, {}, 99);
  assert.equal(result.current, 3);
  assert.equal(result.items.length, 1);
  assert.equal(result.items[0].index, 18);
  assert.equal(M.page(many, {}, -10).current, 0);
});
test('comparison ticks bounded and zero is genuinely zero', () => {
  assert.equal(M.ticks(0, 100), 0);
  assert.equal(M.ticks(1, 100), 1);
  assert.equal(M.ticks(100, 100), 24);
});
test('cancelled and partial errors surfaced', () => {
  const partial = R.validateReport({ ...demo, cancelled: true, suppressed_errors: 2 });
  assert.match(M.warnings(partial), /отменено/);
  assert.match(M.warnings(partial), /неполный/);
  assert.match(M.warnings(partial), /2/);
  assert.equal(M.warnings(demo), '');
});
test('validator rejects invalid schemas, over-limit text, wrong totals', () => {
  assert.throws(() => R.parseReport('{"schema_version":2}'));
  assert.throws(() => R.parseReport(' '.repeat(R.MAX_BYTES + 1)));
  assert.throws(() => R.validateReport({ ...demo, total_bytes: 0 }));
  assert.equal(R.displayText('\u202e<script>'), '\\u202e<script>');
});
test('safe rendering and strict offline CSP contract', () => {
  const app = fs.readFileSync(new URL('app.js', import.meta.url), 'utf8');
  const html = fs.readFileSync(new URL('index.html', import.meta.url), 'utf8');
  const css = fs.readFileSync(new URL('styles.css', import.meta.url), 'utf8');
  assert.doesNotMatch(app, /innerHTML|outerHTML|insertAdjacentHTML|fetch\(/);
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /src="\.\.\/\.\.\/report.js"/);
  assert.doesNotMatch(css, /box-shadow|text-shadow|gradient|backdrop-filter|@import/);
});
