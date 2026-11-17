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
  const document = {
    getElementById: id => ids.get(id) ?? null,
    createElement: tag => new Element(tag),
    createDocumentFragment: () => new Element('#fragment'),
    querySelector: selector => ({ '.category-bars': bars, '.breakdown': breakdown })[selector] ?? null
  };
  const context = vm.createContext({
    document, TextEncoder, Intl, console,
    Option: function (text, value) { const option = new Element('option'); option.textContent = text; option.value = value; return option; }
  });
  for (const file of ['report.js', 'app.js']) {
    vm.runInContext(readFileSync(new URL(file, import.meta.url), 'utf8'), context, { filename: file });
  }
  const demo = JSON.parse(readFileSync(new URL('./demo-report.json', import.meta.url), 'utf8'));
  const findings = Array.from({ length: 31 }, (_, i) => ({
    ...demo.findings[0], rule_id: 'test-' + i, rule_name: 'Расположение ' + String(i).padStart(2, '0'),
    path: '/example/test/' + i, size_bytes: 31 - i, file_count: 1
  }));
  context.fixture = { ...demo, findings, total_bytes: 496, total_files: 31 };
  vm.runInContext("showReport(fixture, false, 'test.json')", context);
  return { ids, context, html };
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
  assert.equal(ids.get('page-label').textContent, '2 / 2');
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
  assert.equal(ids.get('page-label').textContent, '1 / 1');
  assertSelection(ids, 'Расположение 00');
  ids.get('search-input').value = 'not found';
  ids.get('search-input').dispatch('input');
  assert.equal(ids.get('rows').children.length, 0);
  assert.equal(selectedButtons(ids).length, 0);
  assert.equal(ids.get('detail-content').querySelector('h3'), null);
  assert.equal(ids.get('detail-content').textContent, 'Выберите расположение в таблице.');
  assert.equal(ids.get('detail-summary').textContent, 'Расположение не выбрано.');
  assert.equal(ids.get('empty').hidden, false);
});

test('detail announcement is a small polite summary, not the entire panel', () => {
  const { html } = setup();
  assert.match(html, /<p id="detail-summary"[^>]*role="status"[^>]*aria-live="polite"[^>]*aria-atomic="true"/);
  assert.doesNotMatch(html, /<(?:aside|div)[^>]*id="detail-(?:panel|content)"[^>]*aria-live/);
});
