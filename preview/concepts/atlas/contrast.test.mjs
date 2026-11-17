import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
const token = name => css.match(new RegExp(`--${name}:(#[\\da-f]{6})`, 'i'))[1];
function luminance(hex) {
  const rgb = hex.slice(1).match(/../g).map(x => parseInt(x, 16) / 255)
    .map(x => x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
  return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
}
function contrast(a, b) {
  const values = [luminance(a), luminance(b)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}
test('selected category and ribbon text exceed 4.5:1', () => {
  assert.match(css, /\.category\[aria-pressed=true\] small\{color:var\(--ink\)\}/);
  assert.ok(contrast(token('ink'), token('accent')) >= 4.5);
  assert.ok(contrast(token('paper'), '#53574a') >= 4.5);
  for (const background of ['#999c8f', '#c4c7bb', '#a9b09d']) {
    assert.ok(css.includes(`background:${background}`));
    assert.ok(contrast(token('ink'), background) >= 4.5, background);
  }
  for (const nth of ['2n', '3n', '4n']) {
    assert.ok(css.includes(`.ribbon button:nth-child(${nth}) span`));
  }
});
