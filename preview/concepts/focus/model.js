/* Pure view helpers; report validation remains in ../../report.js. */
(function (root) {
  'use strict';
  const PAGE_SIZE = 6;
  function page(findings, options = {}, requested = 0) {
    const matches = root.PruneReport.filterFindings(findings, options);
    const pages = Math.max(1, Math.ceil(matches.length / PAGE_SIZE));
    const current = Math.min(Math.max(0, Math.trunc(requested) || 0), pages - 1);
    return { matches, pages, current, items: matches.slice(current * PAGE_SIZE, (current + 1) * PAGE_SIZE) };
  }
  function ticks(bytes, maximum) {
    if (typeof bytes === 'bigint' || typeof maximum === 'bigint') {
      const size = BigInt(bytes), limit = BigInt(maximum);
      if (!size || !limit) return 0;
      const exactTicks = (size * 24n + limit / 2n) / limit;
      return Number(exactTicks < 1n ? 1n : exactTicks > 24n ? 24n : exactTicks);
    }
    if (!bytes || !maximum) return 0;
    return Math.max(1, Math.round(bytes / maximum * 24));
  }
  function warnings(report) {
    const messages = [];
    if (report.completeness === 'cancelled') messages.push('Сканирование отменено: результаты могут быть неполными.');
    else if (report.completeness === 'partial') messages.push('Отчёт неполный: часть мест могла не попасть в результаты.');
    else if (report.completeness === 'unknown') messages.push('Полнота отчёта неизвестна по данным отчёта.');
    if (report.diagnostic_count) messages.push('Ошибок доступа или чтения: ' + new Intl.NumberFormat('ru-RU').format(report.diagnostic_count) + '.');
    return messages.join(' ');
  }
  root.FocusModel = Object.freeze({ PAGE_SIZE, page, ticks, warnings });
})(globalThis);
