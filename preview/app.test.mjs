import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Adapted from preview/app.test.mjs: run actual scripts without dependencies.
class Element {
  constructor(tag = 'div') {
    this.tagName = tag; this.children = []; this.attributes = {};
    this.listeners = {}; this.dataset = {}; this.style = {};
    this.value = ''; this.hidden = false; this.disabled = false; this._text = '';
  }
  get textContent() { return this._text + this.children.map(c => c.textContent).join(''); }
  set textContent(value) { this._text = String(value); this.children = []; }
  get childElementCount() { return this.children.length; }
  append(...children) { for (const child of children) { child.parentElement = this; this.children.push(child); } }
  replaceChildren(...children) { this.children = []; this._text = ''; this.append(...children); }
  setAttribute(k, v) { this.attributes[k] = String(v); }
  getAttribute(k) { return this.attributes[k]; }
  addEventListener(type, callback) { (this.listeners[type] ??= []).push(callback); }
  async dispatch(type) { await Promise.all((this.listeners[type] ?? []).map(f => f({ target: this }))); }
  click() { if (!this.disabled) return this.dispatch('click'); }
  focus() { this.ownerDocument.activeElement = this; }
  scrollIntoView() {}
}
function setup() {
  const html = readFileSync(new URL('index.html', import.meta.url), 'utf8');
  const ids = new Map([...html.matchAll(/id="([^"]+)"/g)].map(m => [m[1], new Element()]));
  const document = {
    activeElement: { parentElement: null },
    getElementById: id => ids.get(id),
    createElement(tag) { const el = new Element(tag); el.ownerDocument = document; return el; },
    createTextNode(value) { const el = new Element('#text'); el.textContent = value; return el; }
  };
  for (const [id, el] of ids) { el.id = id; el.ownerDocument = document; }
  const context = vm.createContext({ document, TextEncoder, Intl });
  for (const file of ['report.js', 'demo.js', 'app.js']) {
    vm.runInContext(readFileSync(new URL(file, import.meta.url), 'utf8'), context);
  }
  const get = id => ids.get(id);
  const demo = () => JSON.parse(JSON.stringify(context.AtlasDemo));
  const importFile = async (input, name = 'local.json') => {
    get('file-input').files = [{ size: 100, name, text: async () => JSON.stringify(input) }];
    await get('file-input').dispatch('change');
  };
  return { get, document, demo, importFile };
}
const selected = get => get('findings').children.find(row => row.getAttribute('aria-pressed') === 'true');

test('actual app: selection stays stable through search, risk, sort; explicit focus round trip', async () => {
  const { get, document } = setup();
  const row = get('findings').children[1];
  row.focus(); await row.click();
  const index = row.dataset.index;
  assert.equal(document.activeElement, row);
  get('sort').value = 'name'; await get('sort').dispatch('change');
  assert.equal(selected(get).dataset.index, index);
  get('search').value = 'Library'; await get('search').dispatch('input');
  assert.equal(selected(get).dataset.index, index);
  await get('jump-detail').click();
  assert.equal(document.activeElement, get('inspector'));
  await get('back-to-list').click();
  assert.equal(document.activeElement.dataset.index, index);
  get('risk').value = 'never'; await get('risk').dispatch('change');
  assert.equal(get('empty').hidden, false);
  assert.match(get('empty-message').textContent, /условиям/);
  assert.equal(get('jump-detail').disabled, true);
});

test('actual app: empty and cancelled reports differ; unsafe strings are literal text', async () => {
  const { get, demo, importFile } = setup();
  const report = demo(); report.findings = []; report.total_bytes = 0; report.total_files = 0;
  await importFile(report);
  assert.match(get('empty-message').textContent, /В этом отчёте/);
  assert.equal(get('clear-filters').hidden, true);
  report.cancelled = true;
  await importFile(report);
  assert.match(get('empty-message').textContent, /Сканирование отменено/);
  assert.equal(get('report-state').hidden, false);
  assert.match(get('report-state').textContent, /отменено/);
  const unsafe = demo(); unsafe.findings[0].rule_name = '<img src=x onerror=alert(1)>';
  await importFile(unsafe, '<script>.json');
  assert.match(get('findings').textContent, /<img src=x/);
  assert.match(get('source-label').textContent, /<script>/);
});

test('actual app: unknown completeness is explicit and v2 scope avoids root paths', async () => {
  const { get, demo, importFile } = setup();
  const unknownV1 = demo();
  delete unknownV1.partial;
  await importFile(unknownV1, 'unknown-v1.json');
  assert.match(get('report-state').textContent, /Полнота отчёта неизвестна/);
  assert.doesNotMatch(get('report-state').textContent, /Неполный/);
  assert.match(get('inspector-content').textContent, /Неизвестна/);

  const v2 = demo();
  v2.schema_version = 2;
  delete v2.partial;
  delete v2.cancelled;
  v2.completeness = 'unknown';
  v2.scope = {
    root_kind: 'home', comparable: false, min_size_bytes: 4096,
    categories: ['developer'], max_risk: null,
    rule_set_digest: 'sha256:' + 'b'.repeat(64), root_path: '/private/root'
  };
  await importFile(v2, 'v2.json');
  assert.match(get('inspector-content').textContent, /Домашняя папка/);
  assert.match(get('inspector-content').textContent, /Набор правил/);
  assert.match(get('inspector-content').textContent, /КатегорииРазработка/);
  assert.match(get('inspector-content').textContent, /Максимальный риск \(включительно\)/);
  assert.doesNotMatch(get('inspector-content').textContent, /\/private\/root/);
  assert.doesNotMatch(get('inspector-content').textContent, /Сопоставимость/);
  assert.doesNotMatch(get('report-state').textContent, /Неполный/);
  assert.equal(get('inspector-content').children.some(node => node.tagName === 'img'), false);
});

test('actual app: v2 u64 strings render exactly and lossy paths are never presented as exact', async () => {
  const { get, demo, importFile } = setup();
  const base = demo().findings[0];
  const report = demo();
  report.schema_version = 2;
  delete report.partial; delete report.cancelled;
  report.completeness = 'complete';
  report.scanned_at = '18446744073709551615';
  report.findings = [
    { ...base, rule_id: 'large-first', rule_name: 'Large first', path: '/example/first',
      size_bytes: '9007199254740992', file_count: '9007199254740992', last_modified: '18446744073709551615', path_encoding: 'utf8' },
    { ...base, rule_id: 'large-second', rule_name: 'Large second', path: '/example/second',
      size_bytes: '9007199254740993', file_count: '9007199254740993', last_modified: '18446744073709551615', path_encoding: 'lossy_utf8' }
  ];
  report.total_bytes = '18014398509481985';
  report.total_files = '18014398509481985';
  report.suppressed_errors = '0';
  report.scope = {
    root_kind: 'home', comparable: false, min_size_bytes: '9007199254740993',
    categories: ['developer'], max_risk: 'high', rule_set_digest: 'sha256:' + 'd'.repeat(64)
  };
  await importFile(report, 'large-v2.json');
  assert.match(get('total').textContent, /16ПиБ/);
  assert.match(get('scan-date').textContent, /Дата вне диапазона/);
  assert.match(get('inspector-content').textContent, /Дата вне диапазона/);
  assert.match(get('findings').textContent, /Путь преобразован с потерями UTF-8/);
  assert.match(get('inspector-content').textContent, /не используйте эту строку как точный путь/);
});

test('actual app: 10,000 findings mount at most 30; page selection is visible', async () => {
  const { get, demo, importFile } = setup();
  const input = demo();
  input.findings = Array.from({ length: 10000 }, (_, i) => ({
    ...input.findings[0], rule_id: 'item-' + i, path: '/example/' + i, size_bytes: 1, file_count: 1
  }));
  input.total_bytes = 10000; input.total_files = 10000;
  await importFile(input);
  assert.equal(get('findings').children.length, 30);
  assert.equal(get('pagination').hidden, false);
  await get('next-page').click();
  assert.equal(get('findings').children.length, 30);
  assert.equal(selected(get).dataset.index, '30');
  assert.match(get('page-label').textContent, /2 из 334/);
});

test('actual app: latest import wins; invalid import preserves previous report', async () => {
  const { get, demo, importFile } = setup();
  let resolve;
  get('file-input').files = [{ size: 1, name: 'slow.json', text: () => new Promise(r => { resolve = r; }) }];
  const slow = get('file-input').dispatch('change');
  await importFile(demo(), 'latest.json');
  resolve(JSON.stringify(demo())); await slow;
  assert.match(get('source-label').textContent, /latest.json/);
  await importFile({});
  assert.match(get('import-status').textContent, /Предыдущий отчёт сохранён/);
  assert.match(get('source-label').textContent, /latest.json/);
});

test('actual app: category buttons preserve keyboard focus when navigation rebuilds', async () => {
  const { get, document } = setup();
  const category = get('categories').children[1];
  const key = category.dataset.category;
  category.focus(); await category.click();
  assert.equal(document.activeElement.dataset.category, key);
  assert.equal(document.activeElement.getAttribute('aria-pressed'), 'true');
  assert.ok(get('findings').children.length > 0);
  assert.equal(get('ribbon').children.every(segment => segment.tagName === 'span'), true);
  assert.match(get('ribbon').getAttribute('aria-label'), /Иллюстрация/);
});
