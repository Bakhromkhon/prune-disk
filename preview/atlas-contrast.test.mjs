import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
const token = name => css.match(new RegExp(`--${name}:\\s*(#[\\da-f]{6})`, 'i'))[1];
function luminance(hex) {
  const rgb = hex.slice(1).match(/../g).map(x => parseInt(x, 16) / 255)
    .map(x => x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
  return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
}
function contrast(a, b) {
  const values = [luminance(a), luminance(b)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}
test('all text pairings exceed 4.5:1, including selected and secondary labels', () => {
  for (const foreground of ['text', 'text-secondary', 'action']) {
    for (const background of ['surface', 'surface-subtle']) {
      assert.ok(contrast(token(foreground), token(background)) >= 4.5, `${foreground}/${background}`);
    }
  }
  assert.ok(contrast(token('surface'), token('action')) >= 4.5);
  assert.match(css, /\.category\[aria-pressed=true\][^{]*\{[^}]*color: var\(--action\)/);
});
test('control boundaries and focus exceed 3:1 on both surfaces', () => {
  for (const foreground of ['border', 'action']) {
    for (const background of ['surface', 'surface-subtle']) {
      assert.ok(contrast(token(foreground), token(background)) >= 3, `${foreground}/${background}`);
    }
  }
});
test('type floor, compact ribbon and touch target contract', () => {
  for (const match of css.matchAll(/font-size:\s*(\d+)px/g)) assert.ok(Number(match[1]) >= 12);
  assert.match(css, /\.ribbon\s*\{[^}]*height: 16px/);
  assert.match(css, /button, input, select\s*\{[^}]*min-height: 44px/);
  assert.equal([...css.matchAll(/#[\da-f]{6}/gi)].length, 6);
});
