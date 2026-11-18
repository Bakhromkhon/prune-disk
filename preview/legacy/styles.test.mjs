import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
const token = name => {
  const value = css.match(new RegExp(`--${name}:\\s*(#[a-f0-9]{6})`, 'i'))?.[1];
  assert.ok(value, `Missing color token: ${name}`);
  return value;
};
function luminance(hex) {
  const channels = hex.slice(1).match(/../g).map(part => parseInt(part, 16) / 255)
    .map(channel => channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4);
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
}
function contrast(a, b) {
  const values = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (values[0] + .05) / (values[1] + .05);
}

test('placeholder, count and metadata tokens exceed 4.5:1 on their actual surfaces', () => {
  for (const background of [token('surface'), token('bg'), token('tint'), '#eff2ee', '#f0f3ed', '#f0f6f0']) {
    assert.ok(contrast(token('muted'), background) >= 4.5, background);
  }
  assert.match(css, /input::placeholder\s*\{\s*color: var\(--muted\);\s*opacity: 1;/);
  assert.match(css, /\.count\s*\{[^}]*color: var\(--muted\)/);
});

test('control borders exceed 3:1; primary and risk labels exceed 4.5:1', () => {
  for (const background of [token('surface'), token('bg')]) {
    assert.ok(contrast(token('control-border'), background) >= 3);
  }
  for (const [foreground, background] of [
    [token('green'), token('surface')], [token('green'), '#f0f6f0'],
    ['#3a6750', '#f2f7f1'], ['#865b17', '#fff7e8'],
    ['#934332', '#fff0eb'], ['#4e6574', '#f1f5f7'], ['#795519', '#f8eccc']
  ]) {
    assert.ok(contrast(foreground, background) >= 4.5, foreground + ' on ' + background);
  }
});

test('font sizes never fall below 12px and spacing, controls and responsive stacking are explicit', () => {
  const sizes = [...css.matchAll(/font-size:\s*(\d+)px/g)].map(match => Number(match[1]));
  assert.ok(sizes.every(size => size >= 12));
  assert.match(css, /font-size: 14px/);
  assert.match(css, /--space: 24px/);
  assert.match(css, /--space: 16px/);
  assert.match(css, /button\s*\{[^}]*min-height: 44px/);
  assert.match(css, /input, select\s*\{[^}]*min-height: 44px/);
  assert.match(css, /@media \(max-width: 1100px\)\s*\{\s*\.workspace\s*\{\s*grid-template-columns: minmax\(0, 1fr\)/);
  assert.match(css, /\.category-bars, \.filters\s*\{\s*grid-template-columns: minmax\(0, 1fr\)/);
});
