import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Minimal DOM adapter: execute the actual classic scripts and their event handlers,
// without installing packages or touching a browser session.
class Element {
  constructor(tag = 'div') {
    this.tagName = tag;
    this.children = [];
    this.attributes = {};
    this.listeners = {};
    this.className = '';
    this.value = '';
    this.hidden = false;
    this.disabled = false;
    this._text = '';
    this.classList = {
      toggle: (name, active) => {
        const names = new Set(this.className.split(/\s+/).filter(Boolean));
        if (active) names.add(name); else names.delete(name);
        this.className = [...names].join(' ');
      },
      remove: name => this.classList.toggle(name, false),
      add: name => this.classList.toggle(name, true)
    };
  }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
  set textContent(value) { this._text = String(value); this.children = []; }
  get childElementCount() { return this.children.length; }
  append(...children) {
    for (const child of children) {
      if (child.tagName === '#fragment') { this.append(...[...child.children]); continue; }
      if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1);
      this.children.push(child);
      child.parent = this;
    }
  }
  replaceChildren(...children) {
    for (const child of this.children) child.parent = null;
    this.children = []; this._text = '';
    this.append(...children);
  }
  setAttribute(key, value) { this.attributes[key] = String(value); }
  getAttribute(key) { return this.attributes[key]; }
  addEventListener(type, callback) { (this.listeners[type] ??= []).push(callback); }
  dispatch(type) { for (const callback of this.listeners[type] ?? []) callback({ target: this }); }
  click() { if (!this.disabled) this.dispatch('click'); }
  focus() { this.ownerDocument.activeElement = this; }
  querySelector(tag) {
    for (const child of this.children) {
      if (child.tagName === tag) return child;
      const match = child.querySelector(tag);
      if (match) return match;
    }
    return null;
  }
}

function setup() {
  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const ids = new Map([...html.matchAll(/id="([^"]+)"/g)].map(match => [match[1], new Element()]));
  const bars = new Element(), breakdown = new Element();
  const frames = [], readers = [];
  const document = {
    activeElement: null,
    getElementById: id => ids.get(id) ?? null,
    createElement: tag => new Element(tag),
    createDocumentFragment: () => new Element('#fragment'),
    querySelector: selector => ({ '.category-bars': bars, '.breakdown': breakdown })[selector] ?? null
  };
  for (const el of [...ids.values(), bars, breakdown]) el.ownerDocument = document;
  document.createElement = tag => { const el = new Element(tag); el.ownerDocument = document; return el; };
  const context = vm.createContext({
    document, TextEncoder, Intl, console,
    requestAnimationFrame: callback => { frames.push(callback); },
    FileReader: class {
      constructor() { readers.push(this); }
      readAsText(file) { this.result = file.content; }
    },
    Option: function (text, value) { const option = new Element('option'); option.textContent = text; option.value = value; return option; }
  });
  for (const file of ['../report.js', 'app.js']) {
    vm.runInContext(readFileSync(new URL(file, import.meta.url), 'utf8'), context, { filename: file });
  }
  const demo = JSON.parse(readFileSync(new URL('../demo-report.json', import.meta.url), 'utf8'));
  const findings = Array.from({ length: 31 }, (_, i) => ({
    ...demo.findings[0], rule_id: 'test-' + i, rule_name: 'Расположение ' + String(i).padStart(2, '0'),
    path: '/example/test/' + i, size_bytes: 31 - i, file_count: 1
  }));
  context.fixture = { ...demo, findings, total_bytes: 496, total_files: 31 };
  vm.runInContext("showReport(fixture, false, 'test.json')", context);
  function flushFrames() {
    while (frames.length) frames.splice(0).forEach(callback => callback());
  }
  function importFile(file) {
    ids.get('file-input').files = [file];
    ids.get('file-input').dispatch('change');
  }
  return { ids, context, html, bars, document, readers, flushFrames, importFile };
}

function selectedButtons(ids) {
  return ids.get('rows').children.map(row => row.querySelector('button')).filter(button => button.getAttribute('aria-pressed') === 'true');
}
function assertSelection(ids, name) {
  const buttons = selectedButtons(ids);
  assert.equal(buttons.length, 1);
  assert.equal(buttons[0].textContent, name);
  assert.equal(buttons[0].getAttribute('aria-controls'), 'detail-panel');
  assert.equal(ids.get('detail-content').querySelector('h3').textContent, name);
  assert.match(ids.get('detail-summary').textContent, new RegExp(name));
}

test('actual app: 31 findings, next/previous selects a visible row and updates details', () => {
  const { ids } = setup();
  assert.equal(ids.get('rows').children.length, 30);
  assertSelection(ids, 'Расположение 00');
  ids.get('next').click();
  assert.equal(ids.get('rows').children.length, 1);
  assertSelection(ids, 'Расположение 30');
  assert.equal(ids.get('page-label').textContent, 'Страница 2 из 2');
  ids.get('previous').click();
  assert.equal(ids.get('rows').children.length, 30);
  assertSelection(ids, 'Расположение 00');
});

test('actual app: preserves visible selection during filtering and sorting', () => {
  const { ids } = setup();
  ids.get('rows').children[4].querySelector('button').click();
  assertSelection(ids, 'Расположение 04');
  ids.get('risk').value = 'low';
  ids.get('risk').dispatch('change');
  assertSelection(ids, 'Расположение 04');
  ids.get('sort').value = 'name';
  ids.get('sort').dispatch('change');
  assertSelection(ids, 'Расположение 04');
});

test('actual app: filter page reset reconciles selection, empty result clears details', () => {
  const { ids } = setup();
  ids.get('next').click();
  assertSelection(ids, 'Расположение 30');
  ids.get('search-input').value = 'Расположение 0';
  ids.get('search-input').dispatch('input');
  assert.equal(ids.get('page-label').textContent, 'Страница 1 из 1');
  assertSelection(ids, 'Расположение 00');
  ids.get('search-input').value = 'not found';
  ids.get('search-input').dispatch('input');
  assert.equal(ids.get('rows').children.length, 0);
  assert.equal(selectedButtons(ids).length, 0);
  assert.equal(ids.get('detail-content').querySelector('h3'), null);
  assert.equal(ids.get('detail-content').textContent, 'Нет подходящих расположений. Сбросьте фильтры, чтобы вернуть список.');
  assert.equal(ids.get('detail-summary').textContent, 'Расположение не выбрано.');
  assert.equal(ids.get('empty').hidden, false);
});

test('detail announcement is a small polite summary, not the entire panel', () => {
  const { html } = setup();
  assert.match(html, /<p id="detail-summary"[^>]*role="status"[^>]*aria-live="polite"[^>]*aria-atomic="true"/);
  assert.doesNotMatch(html, /<(?:aside|div)[^>]*id="detail-(?:panel|content)"[^>]*aria-live/);
});

test('empty complete, partial and filtered reports have distinct explanations and a working reset', () => {
  const { ids, context, document } = setup();
  context.fixture.findings = [];
  context.fixture.total_bytes = 0;
  context.fixture.total_files = 0;
  vm.runInContext("showReport(fixture, false, 'empty.json')", context);
  assert.match(ids.get('empty').textContent, /В отчёте нет расположений/);
  assert.equal(ids.get('jump-detail').disabled, true);
  assert.equal(ids.get('reset-filters').hidden, true);
  assert.match(ids.get('detail-content').textContent, /нет расположений для просмотра/);
  context.fixture.cancelled = true;
  context.fixture.suppressed_errors = 3;
  vm.runInContext("showReport(fixture, false, 'partial.json')", context);
  assert.match(ids.get('empty').textContent, /Сканирование отменено/);
  assert.match(ids.get('warning').textContent, /^По данным отчёта: Сканирование отменено/);
  assert.match(ids.get('warning').textContent, /3 ошибок без описания/);
  assert.match(ids.get('diagnostic-note').textContent, /Версия схемы: 1/);
  assert.match(ids.get('diagnostic-note').textContent, /3 ошибок описания не записаны/);
  ids.get('demo-button').click();
  ids.get('search-input').value = 'nothing matches';
  ids.get('search-input').dispatch('input');
  assert.match(ids.get('empty').textContent, /Нет совпадений/);
  assert.equal(ids.get('reset-filters').hidden, false);
  ids.get('reset-filters').click();
  assert.equal(ids.get('search-input').value, '');
  assert.equal(ids.get('rows').children.length, 6);
  assert.equal(ids.get('empty').hidden, true);
  assert.equal(document.activeElement, ids.get('search-input'));
});

test('category meters descend by size, share the report denominator, and include percentages', () => {
  const { ids, context, bars } = setup();
  vm.runInContext('showReport(DEMO_REPORT, true)', context);
  const meters = bars.children.map(item => item.querySelector('meter'));
  assert.equal(meters.length, 5);
  assert.deepEqual(meters.map(meter => meter.value), [...meters.map(meter => meter.value)].sort((a, b) => b - a));
  assert.ok(meters.every(meter => meter.max === 10000));
  assert.ok(meters.every(meter => /%/.test(meter.getAttribute('aria-label'))));
  assert.match(bars.children[0].textContent, /^Разработка/);
  assert.match(bars.children[3].textContent, /^Приложения/);
  assert.match(bars.children[4].textContent, /^Система/);
  assert.match(ids.get('source-badge').textContent, /Пример — не данные компьютера/);
});

test('selection does not steal focus; explicit detail jumps return to their initiating button', () => {
  const { ids, document } = setup();
  const name = ids.get('rows').children[2].querySelector('button');
  name.focus();
  name.click();
  assert.equal(document.activeElement, name);
  ids.get('jump-detail').focus();
  ids.get('jump-detail').click();
  assert.equal(document.activeElement, ids.get('detail-title'));
  ids.get('back-to-row').click();
  assert.equal(document.activeElement, ids.get('jump-detail'));
  const jump = ids.get('rows').children[5].children[0].children.find(child => child.className === 'row-jump');
  assert.equal(jump.getAttribute('aria-controls'), 'detail-panel');
  jump.click();
  assertSelection(ids, 'Расположение 05');
  assert.equal(document.activeElement, ids.get('detail-title'));
  ids.get('back-to-row').click();
  assert.equal(document.activeElement, jump);
});

test('pagination announces full page text and moves focus away from newly disabled controls', () => {
  const { ids, document, html } = setup();
  ids.get('next').focus();
  ids.get('next').click();
  assert.equal(ids.get('next').disabled, true);
  assert.equal(ids.get('page-label').textContent, 'Страница 2 из 2');
  assert.equal(document.activeElement, ids.get('page-label'));
  ids.get('previous').focus();
  ids.get('previous').click();
  assert.equal(document.activeElement, ids.get('page-label'));
  assert.match(html, /id="page-label"[^>]*role="status"[^>]*aria-live="polite"/);
});

test('three-page navigation preserves focus while the clicked control is still enabled', () => {
  const { ids, document, context } = setup();
  context.fixture.findings = Array.from({ length: 61 }, (_, index) => ({
    ...context.fixture.findings[0], rule_id: 'page-' + index, rule_name: 'Item ' + index,
    size_bytes: 1, file_count: 1
  }));
  context.fixture.total_bytes = 61;
  context.fixture.total_files = 61;
  vm.runInContext("showReport(fixture, false, 'three-pages.json')", context);
  ids.get('next').focus();
  ids.get('next').click();
  assert.equal(ids.get('page-label').textContent, 'Страница 2 из 3');
  assert.equal(document.activeElement, ids.get('next'));
  assertSelection(ids, 'Item 30');
  ids.get('next').click();
  assert.equal(ids.get('page-label').textContent, 'Страница 3 из 3');
  assert.equal(document.activeElement, ids.get('page-label'));
  assertSelection(ids, 'Item 60');
});

test('zero-byte categories expose zero percent without invalid meter bounds', () => {
  const { context, bars } = setup();
  context.fixture.findings = [{ ...context.fixture.findings[0], size_bytes: 0 }];
  context.fixture.total_bytes = 0;
  context.fixture.total_files = 1;
  vm.runInContext("showReport(fixture, false, 'zero.json')", context);
  const meter = bars.children[0].querySelector('meter');
  assert.equal(meter.value, 0);
  assert.equal(meter.max, 10000);
  assert.match(meter.getAttribute('aria-label'), /0%/);
  assert.doesNotMatch(bars.textContent, /NaN|Infinity/);
});

test('invalid JSON retains the report, gives an action, and repeats its live announcement', () => {
  const { ids, readers, importFile, flushFrames } = setup();
  for (let attempt = 0; attempt < 2; attempt++) {
    importFile({ name: 'bad.json', size: 1, content: '{' });
    readers.at(-1).onload();
    assert.equal(ids.get('import-status').textContent, '');
    assert.equal(ids.get('import-error-detail').hidden, false);
    assert.match(ids.get('import-error-message').textContent, /корректным JSON/);
    flushFrames();
    assert.match(ids.get('import-status').textContent, /Выберите JSON-отчёт Prune версии 1/);
    assert.match(ids.get('import-status').textContent, /Текущий отчёт сохранён/);
    assertSelection(ids, 'Расположение 00');
  }
});

test('schema import errors retain the existing report and keep technical names out of the primary action', () => {
  const { ids, context, readers, importFile, flushFrames } = setup();
  const fixture = { ...context.fixture, schema_version: 3 };
  importFile({ name: 'unsupported.json', size: 1024, content: JSON.stringify(fixture) });
  readers.at(-1).onload();
  flushFrames();
  assert.match(ids.get('import-error-message').textContent, /schema_version/);
  assert.doesNotMatch(ids.get('import-status').textContent, /schema_version/);
  assertSelection(ids, 'Расположение 00');
});

test('v2 context is explicit, safe and independent of root identity', () => {
  const { ids, context } = setup();
  const fixture = { ...context.fixture, schema_version: 2 };
  delete fixture.partial;
  delete fixture.cancelled;
  fixture.completeness = 'unknown';
  fixture.scope = {
    root_kind: 'custom', comparable: false, min_size_bytes: 0,
    categories: ['developer'], max_risk: null,
    rule_set_digest: 'sha256:' + 'c'.repeat(64), root_path: '/private/root'
  };
  context.fixture = fixture;
  vm.runInContext("showReport(fixture, false, 'v2.json')", context);
  assert.match(ids.get('warning').textContent, /полнота отчёта неизвестна/i);
  assert.doesNotMatch(ids.get('warning').textContent, /отчёт неполный/i);
  assert.match(ids.get('scan-scope').textContent, /путь не указан/);
  assert.match(ids.get('scan-scope').textContent, /КатегорииРазработка/);
  assert.match(ids.get('scan-scope').textContent, /Максимальный риск \(включительно\)/);
  assert.doesNotMatch(ids.get('scan-scope').textContent, /\/private\/root/);
  assert.doesNotMatch(ids.get('scan-scope').textContent, /Сопоставимость/);
  assert.equal(ids.get('scan-scope').children.some(node => node.tagName === 'img'), false);
});

test('legacy view renders v2 u64 strings, bounded bars, out-of-range dates and lossy-path warnings', () => {
  const { ids, context, bars } = setup();
  const base = context.fixture.findings[0];
  const fixture = { ...context.fixture, schema_version: 2, completeness: 'complete' };
  delete fixture.partial; delete fixture.cancelled;
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
    categories: ['developer'], max_risk: 'high', rule_set_digest: 'sha256:' + 'e'.repeat(64)
  };
  context.fixture = fixture;
  vm.runInContext("showReport(fixture, false, 'large-v2.json')", context);
  assert.match(ids.get('total-bytes').textContent, /16 ПиБ/);
  assert.match(ids.get('report-meta').textContent, /Дата вне диапазона/);
  assert.equal(bars.children.length, 1);
  assert.equal(bars.children[0].querySelector('meter').max, 10000);
  assert.match(ids.get('rows').textContent, /Путь преобразован с потерями UTF-8/);
  assert.match(ids.get('detail-content').textContent, /не используйте эту строку как точный путь/);
  assert.match(ids.get('detail-content').textContent, /Дата вне диапазона/);
});

test('import read and size failures give specific next steps; technical details remain literal text', () => {
  const { ids, readers, importFile, flushFrames } = setup();
  importFile({ name: 'huge.json', size: 5 * 1024 * 1024 + 1 });
  assert.equal(readers.length, 0);
  flushFrames();
  assert.match(ids.get('import-status').textContent, /не больше 5 МиБ/);
  importFile({ name: 'unreadable.json', size: 1 });
  readers.at(-1).onerror();
  flushFrames();
  assert.match(ids.get('import-status').textContent, /Проверьте доступ/);
  importFile({ name: 'broken.json', size: 1 });
  readers.at(-1).onload();
  // Exercise the actual error handler with a hostile exception, not an HTML sink.
  const originalMessage = ids.get('import-error-message').textContent;
  assert.ok(originalMessage.length > 0);
  Object.defineProperty(readers.at(-1), 'result', { get() { throw new Error('<script>\n\u202e'); } });
  readers.at(-1).onload();
  assert.equal(ids.get('import-error-message').textContent, '<script>\\u000a\\u202e');
  assert.equal(ids.get('import-error-message').childElementCount, 0);
  assertSelection(ids, 'Расположение 00');
});

test('successful import escapes filename and findings; a newer choice wins over stale reads', () => {
  const { ids, context, readers, importFile, flushFrames } = setup();
  importFile({ name: 'stale.json', size: 1, content: '{' });
  const stale = readers.at(-1);
  const fixture = JSON.parse(JSON.stringify(context.fixture));
  fixture.findings[0].rule_name = '<img src=x>\n\u202e';
  importFile({ name: '<b>report</b>\n.json', size: 1024, content: JSON.stringify(fixture) });
  readers.at(-1).onload();
  flushFrames();
  assert.equal(ids.get('source-badge').textContent, 'Открытый файл');
  assert.equal(ids.get('source-name').textContent, '<b>report</b>\\u000a.json');
  assert.equal(ids.get('detail-content').querySelector('h3').textContent, '<img src=x>\\u000a\\u202e');
  assert.equal(ids.get('import-error-detail').hidden, true);
  stale.onload();
  flushFrames();
  assert.match(ids.get('import-status').textContent, /Отчёт открыт локально/);
});

test('viewer retains local-only CSP and provides real CLI help without export or deletion controls', () => {
  const { html } = setup();
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /script-src 'self'; style-src 'self'/);
  assert.match(html, /prune scan --json &gt; report\.json/);
  assert.match(html, /простого экспорта через интерфейс ещё нет/);
  assert.match(html, /не сканирует компьютер и не удаляет файлы/);
  assert.doesNotMatch(html, /(?:src|href)="https?:/);
});
