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

class FakeElement {
  constructor(tag = 'div') {
    this.tagName = tag; this.children = []; this.attributes = {}; this.listeners = {};
    this.style = {}; this.value = ''; this.hidden = false; this.disabled = false; this._text = '';
  }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
  set textContent(value) { this._text = String(value); this.children = []; }
  getAttribute(key) { return this.attributes[key]; }
  append(...children) {
    for (const child of children) {
      if (child.parentElement) child.parentElement.children.splice(child.parentElement.children.indexOf(child), 1);
      child.parentElement = this; this.children.push(child);
    }
  }
  replaceChildren(...children) { this.children = []; this._text = ''; this.append(...children); }
  setAttribute(key, value) { this.attributes[key] = String(value); }
  addEventListener(type, callback) { (this.listeners[type] ??= []).push(callback); }
  async dispatch(type) { for (const callback of this.listeners[type] ?? []) await callback({ target: this }); }
  focus() {}
  scrollIntoView() {}
}

function setupFocusApp() {
  const html = fs.readFileSync(new URL('index.html', import.meta.url), 'utf8');
  const ids = new Map([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], new FakeElement()]));
  const document = {
    getElementById: id => ids.get(id),
    createElement: tag => new FakeElement(tag),
    createTextNode(value) { const node = new FakeElement('#text'); node.textContent = value; return node; }
  };
  const appContext = vm.createContext({ document, TextEncoder, Intl, window: { matchMedia: () => ({ matches: false }) } });
  for (const file of ['../../report.js', 'model.js', 'demo.js', 'app.js']) {
    vm.runInContext(fs.readFileSync(new URL(file, import.meta.url), 'utf8'), appContext);
  }
  return { ids, appContext };
}

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
  assert.equal(M.ticks(9007199254740993n, 18014398509481985n), 12);
  assert.equal(M.ticks(1n, 18446744073709551615n), 1);
  assert.equal(M.ticks(18446744073709551615n, 1n), 24);
});
test('cancelled, partial and unknown completeness messages are distinct', () => {
  const partial = R.validateReport({ ...demo, cancelled: true, suppressed_errors: 2 });
  assert.match(M.warnings(partial), /отменено/);
  assert.match(M.warnings(partial), /неполными/);
  assert.match(M.warnings(partial), /2/);
  const incompleteInput = { ...context.FocusDemo };
  incompleteInput.partial = true;
  assert.match(M.warnings(R.validateReport(incompleteInput)), /Отчёт неполный/);
  const unknownInput = { ...context.FocusDemo };
  delete unknownInput.partial;
  const unknownWarnings = M.warnings(R.validateReport(unknownInput));
  assert.match(unknownWarnings, /Полнота отчёта неизвестна/);
  assert.doesNotMatch(unknownWarnings, /Отчёт неполный/);
  assert.equal(M.warnings(demo), '');
});
test('validator rejects invalid schemas, over-limit text, wrong totals', () => {
  assert.throws(() => R.parseReport('{"schema_version":3}'));
  assert.throws(() => R.parseReport(' '.repeat(R.MAX_BYTES + 1)));
  assert.throws(() => R.validateReport({ ...demo, total_bytes: 0 }));
  assert.equal(R.displayText('\u202e<script>'), '\\u202e<script>');
});
test('safe rendering and strict offline CSP contract', () => {
  const app = fs.readFileSync(new URL('app.js', import.meta.url), 'utf8');
  const html = fs.readFileSync(new URL('index.html', import.meta.url), 'utf8');
  const css = fs.readFileSync(new URL('styles.css', import.meta.url), 'utf8');
  assert.doesNotMatch(app, /innerHTML|outerHTML|insertAdjacentHTML|fetch\(/);
  assert.match(app, /f\.path_encoding === 'lossy_utf8'/);
  assert.match(app, /R\.lossyPathWarning/);
  assert.match(app, /Путь преобразован с потерями UTF-8/);
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /src="\.\.\/\.\.\/report.js"/);
  assert.doesNotMatch(css, /box-shadow|text-shadow|gradient|backdrop-filter|@import/);
});

test('actual Focus app renders v2 u64 strings and warns on a lossy path', async () => {
  const { ids, appContext } = setupFocusApp();
  const fixture = JSON.parse(JSON.stringify(context.FocusDemo));
  const base = fixture.findings[0];
  fixture.schema_version = 2;
  delete fixture.partial; delete fixture.cancelled;
  fixture.completeness = 'complete';
  fixture.scanned_at = '18446744073709551615';
  fixture.findings = [
    { ...base, rule_id: 'large-first', rule_name: 'Large first', path: '/example/first',
      size_bytes: '9007199254740992', file_count: '9007199254740992', last_modified: '18446744073709551615', path_encoding: 'lossy_utf8' },
    { ...base, rule_id: 'large-second', rule_name: 'Large second', path: '/example/second',
      size_bytes: '9007199254740993', file_count: '9007199254740993', last_modified: '18446744073709551615', path_encoding: 'utf8' }
  ];
  fixture.total_bytes = '18014398509481985';
  fixture.total_files = '18014398509481985';
  fixture.suppressed_errors = '0';
  fixture.scope = {
    root_kind: 'home', comparable: false, min_size_bytes: '9007199254740993',
    categories: ['developer'], max_risk: 'high', rule_set_digest: 'sha256:' + 'f'.repeat(64)
  };
  assert.equal(R.validateReport(fixture).findings.length, 2);
  assert.equal(appContext.PruneReport.validateReport(fixture).findings.length, 2);
  const serialized = JSON.stringify(fixture);
  assert.equal(appContext.PruneReport.parseReport(serialized).findings.length, 2);
  const fileInput = ids.get('file-input');
  fileInput.files = [{ size: 1024, name: 'large.json', text: async () => serialized }];
  await fileInput.dispatch('change');
  assert.match(ids.get('import-status').textContent, /Файл открыт локально/);
  assert.match(ids.get('summary').textContent, /16 ПиБ/);
  assert.match(ids.get('report-meta').textContent, /Дата вне диапазона/);
  assert.match(ids.get('list').textContent, /Путь преобразован с потерями UTF-8/);
  assert.ok(ids.get('list').children.some(button => /Путь отображён с потерями UTF-8/.test(button.getAttribute('aria-label'))));
  assert.match(ids.get('detail').textContent, /не используйте эту строку как точный путь/);
  assert.match(ids.get('detail').textContent, /Дата вне диапазона/);
});
