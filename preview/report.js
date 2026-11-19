/* Shared pure functions; classic script keeps file:// usable without module fetches. */
(function (root) {
  'use strict';
  const categories = Object.freeze({ developer: 'Разработка', browser: 'Браузеры', system: 'Система', trash: 'Корзина', apps: 'Приложения', media: 'Медиа', other: 'Другое' });
  const risks = Object.freeze({ safe: 'Минимальный', low: 'Низкий', medium: 'Средний', high: 'Высокий', never: 'Не удалять' });
  const recreates = Object.freeze({ automatic: 'Автоматически', download: 'Повторная загрузка', rebuild: 'Повторная сборка', manual: 'Вручную', impossible: 'Невозможно', unknown: 'Неизвестно' });
  const pathEncodings = Object.freeze({ utf8: true, lossy_utf8: true });
  const lossyPathWarning = 'Путь показан с потерями преобразования UTF-8. Он может не совпадать с исходными POSIX-байтами; не используйте эту строку как точный путь или для файловых операций.';
  const completenessLabels = Object.freeze({ complete: 'Полный по данным отчёта', partial: 'Неполный', cancelled: 'Сканирование отменено', unknown: 'Неизвестна' });
  const rootKinds = Object.freeze({ home: 'Домашняя папка', custom: 'Пользовательская область (путь не указан)', empty: 'Пустой набор' });
  const MAX_BYTES = 5 * 1024 * 1024;
  const U64_MAX = (1n << 64n) - 1n;
  const MAX_DATE_SECONDS = 8640000000000n;
  const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
  function fail(field) { throw new Error('Некорректное поле отчёта: ' + field); }
  function object(v, field) { if (!v || typeof v !== 'object' || Array.isArray(v)) fail(field); return v; }
  function text(v, field, max = 8192) { if (typeof v !== 'string' || v.length > max) fail(field); return v; }
  function number(v, field) { if (!Number.isSafeInteger(v) || v < 0) fail(field); return v; }
  function unsigned(v, field, schemaVersion) {
    if (schemaVersion === 1) return number(v, field);
    if (typeof v === 'bigint') {
      if (v < 0n || v > U64_MAX) fail(field);
      return v;
    }
    if (typeof v === 'number') return BigInt(number(v, field));
    if (typeof v !== 'string' || v.length > 20 || !/^(0|[1-9][0-9]*)$/.test(v)) fail(field);
    const exact = BigInt(v);
    if (exact > U64_MAX) fail(field);
    return exact;
  }
  function bool(v, field) { if (v === undefined) return false; if (typeof v !== 'boolean') fail(field); return v; }
  function optionalBool(o, key, field) {
    if (!own(o, key)) return undefined;
    if (o[key] === undefined) fail(field);
    return bool(o[key], field);
  }
  function choice(v, map, field) { if (typeof v !== 'string' || !own(map, v)) fail(field); return v; }
  function accounting(v) { if (v !== undefined && v !== 'logical') fail('byte_accounting'); return 'logical'; }
  function validateScope(value, schemaVersion) {
    const s = object(value, 'scope');
    if (typeof s.comparable !== 'boolean') fail('scope.comparable');
    const rootKind = choice(s.root_kind, rootKinds, 'scope.root_kind');
    if (s.comparable !== false) fail('scope.comparable (root identity is not public)');
    if (!Array.isArray(s.categories) || s.categories.length > 128) fail('scope.categories (максимум 128)');
    const seenCategories = new Set();
    const scopeCategories = Array.from(s.categories, (value, i) => {
      const category = choice(value, categories, 'scope.categories[' + i + ']');
      if (seenCategories.has(category)) fail('scope.categories[' + i + '] (повтор)');
      seenCategories.add(category);
      return category;
    });
    const sortedCategories = [...scopeCategories].sort();
    if (sortedCategories.some((category, index) => category !== scopeCategories[index])) {
      fail('scope.categories (порядок)');
    }
    let maxRisk = null;
    if (s.max_risk !== null) {
      maxRisk = choice(s.max_risk, risks, 'scope.max_risk');
    }
    const ruleSetDigest = text(s.rule_set_digest, 'scope.rule_set_digest', 71);
    if (!/^sha256:[0-9a-f]{64}$/.test(ruleSetDigest)) fail('scope.rule_set_digest');
    return {
      root_kind: rootKind,
      comparable: s.comparable,
      min_size_bytes: unsigned(s.min_size_bytes, 'scope.min_size_bytes', schemaVersion),
      categories: scopeCategories,
      max_risk: maxRisk,
      rule_set_digest: ruleSetDigest
    };
  }
  function validateReport(input) {
    const r = object(input, 'report');
    const schemaVersion = r.schema_version;
    if (schemaVersion !== 1 && schemaVersion !== 2) fail('schema_version (поддерживаются 1 и 2)');
    if (!Array.isArray(r.findings) || r.findings.length > 10000) fail('findings (максимум 10 000)');
    if (!Array.isArray(r.errors) || r.errors.length > 1000) fail('errors (максимум 1 000)');
    const findings = Array.from(r.findings, (value, i) => {
      const f = object(value, 'findings[' + i + ']');
      return {
        rule_id: text(f.rule_id, 'rule_id', 256), rule_name: text(f.rule_name, 'rule_name', 512),
        category: choice(f.category, categories, 'category'), risk: choice(f.risk, risks, 'risk'),
        recreate: choice(f.recreate, recreates, 'recreate'), path: text(f.path, 'path'),
        path_encoding: f.path_encoding === undefined ? 'utf8' : choice(f.path_encoding, pathEncodings, 'finding.path_encoding'),
        size_bytes: unsigned(f.size_bytes, 'size_bytes', schemaVersion), file_count: unsigned(f.file_count, 'file_count', schemaVersion),
        last_modified: f.last_modified === null ? null : unsigned(f.last_modified, 'last_modified', schemaVersion),
        description: text(f.description, 'description'), byte_accounting: accounting(f.byte_accounting),
        partial: bool(f.partial, 'finding.partial')
      };
    });
    const errors = Array.from(r.errors, e => {
      object(e, 'error');
      return { path: text(e.path, 'error.path'), message: text(e.message, 'error.message') };
    });
    const rawCancelled = optionalBool(r, 'cancelled', 'cancelled');
    const rawPartial = optionalBool(r, 'partial', 'partial');
    const cancelled = rawCancelled === true;
    const zero = schemaVersion === 2 ? 0n : 0;
    const suppressed_errors = r.suppressed_errors === undefined ? zero : unsigned(r.suppressed_errors, 'suppressed_errors', schemaVersion);
    const diagnostic_count = schemaVersion === 2 ?
      BigInt(errors.length) + suppressed_errors : number(errors.length + suppressed_errors, 'сумма errors и suppressed_errors');
    let bytes = zero, files = zero;
    for (const f of findings) {
      bytes += f.size_bytes; files += f.file_count;
      if (schemaVersion === 1) {
        number(bytes, 'сумма size_bytes'); number(files, 'сумма file_count');
      }
    }
    const total_bytes = unsigned(r.total_bytes, 'total_bytes', schemaVersion), total_files = unsigned(r.total_files, 'total_files', schemaVersion);
    if (bytes !== total_bytes || files !== total_files) fail('итоги не совпадают с findings');
    let completeness, scope = null;
    if (schemaVersion === 1) {
      const hasPartialEvidence = rawPartial === true || errors.length > 0 || suppressed_errors > 0 || findings.some(f => f.partial);
      completeness = cancelled ? 'cancelled' : hasPartialEvidence ? 'partial' : rawPartial === false ? 'complete' : 'unknown';
    } else {
      completeness = choice(r.completeness, completenessLabels, 'completeness');
      scope = validateScope(r.scope, schemaVersion);
      if (rawCancelled !== undefined && rawCancelled !== (completeness === 'cancelled')) fail('completeness/cancelled');
      if (rawPartial !== undefined && rawPartial !== (completeness === 'partial')) fail('completeness/partial');
      const hasPartialEvidence = errors.length > 0 || suppressed_errors !== zero || findings.some(f => f.partial);
      if (hasPartialEvidence && (completeness === 'complete' || completeness === 'unknown')) {
        fail('completeness (противоречит признакам неполноты)');
      }
    }
    const result = {
      schema_version: schemaVersion, platform: text(r.platform, 'platform', 128), scanned_at: unsigned(r.scanned_at, 'scanned_at', schemaVersion),
      total_bytes, total_files, findings, errors, byte_accounting: accounting(r.byte_accounting),
      completeness, suppressed_errors, diagnostic_count
    };
    if (schemaVersion === 1) {
      result.cancelled = cancelled;
      // Keep old consumers conservative: unknown is never collapsed into "complete".
      result.partial = completeness !== 'complete';
    } else {
      result.scope = scope;
    }
    return result;
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
    if (typeof value === 'bigint') {
      if (!value) return '0 Б';
      const units = ['Б', 'КиБ', 'МиБ', 'ГиБ', 'ТиБ', 'ПиБ'];
      let unit = 0;
      while (unit < units.length - 1 && value >= 1024n ** BigInt(unit + 1)) unit++;
      if (!unit) return new Intl.NumberFormat('ru-RU').format(value) + ' Б';
      const factor = 1024n ** BigInt(unit);
      const tenths = (value * 10n + factor / 2n) / factor;
      const display = Number(tenths) / 10;
      return new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 1 }).format(display) + ' ' + units[unit];
    }
    if (!value) return '0 Б';
    const units = ['Б', 'КиБ', 'МиБ', 'ГиБ', 'ТиБ', 'ПиБ'];
    const unit = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
    return new Intl.NumberFormat('ru-RU', { maximumFractionDigits: unit ? 1 : 0 }).format(value / 1024 ** unit) + ' ' + units[unit];
  }
  function sumIntegers(values, initial = 0) {
    let total = initial;
    for (const value of values) {
      if (typeof total === 'bigint' || typeof value === 'bigint') {
        total = BigInt(total) + BigInt(value);
      } else {
        total += value;
        number(total, 'сумма целых чисел');
      }
    }
    return total;
  }
  function compareIntegers(a, b) { return a < b ? -1 : a > b ? 1 : 0; }
  function ratioBasisPoints(value, total) {
    if (typeof value === 'bigint' || typeof total === 'bigint') {
      const n = BigInt(value), d = BigInt(total);
      if (!n || !d) return 0;
      const ratio = (n * 10000n + d / 2n) / d;
      return Number(ratio > 10000n ? 10000n : ratio);
    }
    if (!total) return 0;
    return Math.max(0, Math.min(10000, Math.round(value / total * 10000)));
  }
  function percentage(value, total, fractionDigits = 0) {
    const digits = Math.min(3, Math.max(0, Math.trunc(fractionDigits) || 0));
    const scale = 10 ** digits;
    if (typeof value === 'bigint' || typeof total === 'bigint') {
      const n = BigInt(value), d = BigInt(total);
      if (!d) return 0;
      const factor = 100n * BigInt(scale);
      const rounded = (n * factor + d / 2n) / d;
      return Number(rounded) / scale;
    }
    return total ? Math.round(value / total * 100 * scale) / scale : 0;
  }
  function dateFromSeconds(value) {
    if (typeof value === 'bigint') {
      if (value > MAX_DATE_SECONDS) return null;
      value = Number(value);
    }
    const date = new Date(value * 1000);
    return Number.isNaN(date.getTime()) ? null : date;
  }
  function scopeDetails(scope) {
    const categoryNames = scope.categories.map(value => own(categories, value) ? categories[value] : value);
    const maxRisk = scope.max_risk === null ? 'Не задан' : risks[scope.max_risk];
    return [
      ['Тип корня', rootKinds[scope.root_kind]],
      ['Минимальный размер', formatBytes(scope.min_size_bytes) + ' (' + new Intl.NumberFormat('ru-RU').format(scope.min_size_bytes) + ' Б)'],
      ['Категории', categoryNames.length ? categoryNames.join(', ') : 'Не указаны'],
      ['Максимальный риск (включительно)', maxRisk],
      ['Набор правил', scope.rule_set_digest]
    ];
  }
  function filterFindings(findings, { category = '', risk = '', query = '', sort = 'size' } = {}) {
    const needle = query.toLocaleLowerCase('ru');
    const rank = Object.keys(risks);
    return findings.map((f, index) => ({ ...f, index })).filter(f =>
      (!category || f.category === category) && (!risk || f.risk === risk) &&
      (!needle || (f.path + ' ' + f.rule_name + ' ' + f.description).toLocaleLowerCase('ru').includes(needle) ||
        displayText(f.path + ' ' + f.rule_name + ' ' + f.description).toLocaleLowerCase('ru').includes(needle)))
    .sort((a, b) => (sort === 'name' ? a.rule_name.localeCompare(b.rule_name, 'ru') :
        sort === 'risk' ? rank.indexOf(b.risk) - rank.indexOf(a.risk) : compareIntegers(b.size_bytes, a.size_bytes)) || a.index - b.index);
  }
  root.PruneReport = Object.freeze({ categories, risks, recreates, pathEncodings, lossyPathWarning, completenessLabels, rootKinds, MAX_BYTES, validateReport, parseReport, displayText, formatBytes, sumIntegers, compareIntegers, ratioBasisPoints, percentage, dateFromSeconds, scopeDetails, filterFindings });
})(globalThis);
