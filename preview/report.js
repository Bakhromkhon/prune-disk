/* Shared pure functions; classic script keeps file:// usable without module fetches. */
(function (root) {
  'use strict';
  const categories = Object.freeze({ developer: 'Разработка', browser: 'Браузеры', system: 'Система', trash: 'Корзина', apps: 'Приложения', media: 'Медиа', other: 'Другое' });
  const risks = Object.freeze({ safe: 'Безопасный', low: 'Низкий', medium: 'Средний', high: 'Высокий', never: 'Не удалять' });
  const recreates = Object.freeze({ automatic: 'Автоматически', download: 'Повторная загрузка', rebuild: 'Повторная сборка', manual: 'Вручную', impossible: 'Невозможно', unknown: 'Неизвестно' });
  const MAX_BYTES = 5 * 1024 * 1024;
  const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
  function fail(field) { throw new Error('Некорректное поле отчёта: ' + field); }
  function object(v, field) { if (!v || typeof v !== 'object' || Array.isArray(v)) fail(field); return v; }
  function text(v, field, max = 8192) { if (typeof v !== 'string' || v.length > max) fail(field); return v; }
  function number(v, field) { if (!Number.isSafeInteger(v) || v < 0) fail(field); return v; }
  function bool(v, field) { if (v === undefined) return false; if (typeof v !== 'boolean') fail(field); return v; }
  function choice(v, map, field) { if (typeof v !== 'string' || !own(map, v)) fail(field); return v; }
  function accounting(v) { if (v !== undefined && v !== 'logical') fail('byte_accounting'); return 'logical'; }
  function validateReport(input) {
    const r = object(input, 'report');
    if (r.schema_version !== 1) fail('schema_version (поддерживается только 1)');
    if (!Array.isArray(r.findings) || r.findings.length > 10000) fail('findings (максимум 10 000)');
    if (!Array.isArray(r.errors) || r.errors.length > 1000) fail('errors (максимум 1 000)');
    const findings = r.findings.map((value, i) => {
      const f = object(value, 'findings[' + i + ']');
      return {
        rule_id: text(f.rule_id, 'rule_id', 256), rule_name: text(f.rule_name, 'rule_name', 512),
        category: choice(f.category, categories, 'category'), risk: choice(f.risk, risks, 'risk'),
        recreate: choice(f.recreate, recreates, 'recreate'), path: text(f.path, 'path'),
        size_bytes: number(f.size_bytes, 'size_bytes'), file_count: number(f.file_count, 'file_count'),
        last_modified: f.last_modified === null ? null : number(f.last_modified, 'last_modified'),
        description: text(f.description, 'description'), byte_accounting: accounting(f.byte_accounting),
        partial: bool(f.partial, 'finding.partial')
      };
    });
    const errors = r.errors.map(e => {
      object(e, 'error');
      return { path: text(e.path, 'error.path'), message: text(e.message, 'error.message') };
    });
    const cancelled = bool(r.cancelled, 'cancelled');
    const suppressed_errors = number(r.suppressed_errors === undefined ? 0 : r.suppressed_errors, 'suppressed_errors');
    let bytes = 0, files = 0;
    for (const f of findings) {
      bytes += f.size_bytes; files += f.file_count;
      number(bytes, 'сумма size_bytes'); number(files, 'сумма file_count');
    }
    const total_bytes = number(r.total_bytes, 'total_bytes'), total_files = number(r.total_files, 'total_files');
    if (bytes !== total_bytes || files !== total_files) fail('итоги не совпадают с findings');
    return {
      schema_version: 1, platform: text(r.platform, 'platform', 128), scanned_at: number(r.scanned_at, 'scanned_at'),
      total_bytes, total_files, findings, errors, byte_accounting: accounting(r.byte_accounting),
      cancelled, suppressed_errors,
      partial: bool(r.partial, 'partial') || cancelled || errors.length > 0 || suppressed_errors > 0 || findings.some(f => f.partial)
    };
  }
  function parseReport(source) {
    if (typeof source !== 'string' || new TextEncoder().encode(source).length > MAX_BYTES) throw new Error('Размер JSON превышает 5 МиБ.');
    let value;
    try { value = JSON.parse(source); } catch { throw new Error('Файл не является корректным JSON.'); }
    return validateReport(value);
  }
  function displayText(value) {
    return String(value).replace(/[\\\u0000-\u001f\u007f-\u009f\u061c\u200e\u200f\u202a-\u202e\u2066-\u2069]/g,
      c => c === '\\' ? '\\\\' : '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'));
  }
  function formatBytes(value) {
    if (!value) return '0 Б';
    const units = ['Б', 'КиБ', 'МиБ', 'ГиБ', 'ТиБ', 'ПиБ'];
    const unit = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
    return new Intl.NumberFormat('ru-RU', { maximumFractionDigits: unit ? 1 : 0 }).format(value / 1024 ** unit) + ' ' + units[unit];
  }
  function filterFindings(findings, { category = '', risk = '', query = '', sort = 'size' } = {}) {
    const needle = query.toLocaleLowerCase('ru');
    const rank = Object.keys(risks);
    return findings.map((f, index) => ({ ...f, index })).filter(f =>
      (!category || f.category === category) && (!risk || f.risk === risk) &&
      (!needle || displayText(f.path + ' ' + f.rule_name + ' ' + f.description).toLocaleLowerCase('ru').includes(needle)))
      .sort((a, b) => (sort === 'name' ? a.rule_name.localeCompare(b.rule_name, 'ru') :
        sort === 'risk' ? rank.indexOf(b.risk) - rank.indexOf(a.risk) : b.size_bytes - a.size_bytes) || a.index - b.index);
  }
  root.PruneReport = Object.freeze({ categories, risks, recreates, MAX_BYTES, validateReport, parseReport, displayText, formatBytes, filterFindings });
})(globalThis);
