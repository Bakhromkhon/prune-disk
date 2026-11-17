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
    if (!bytes || !maximum) return 0;
    return Math.max(1, Math.round(bytes / maximum * 24));
  }
  function warnings(report) {
    const messages = [];
    if (report.cancelled) messages.push('Сканирование в отчёте отменено.');
    if (report.partial) messages.push('Отчёт неполный: часть мест могла не попасть в результаты.');
    if (report.errors.length || report.suppressed_errors) messages.push('Ошибок доступа или чтения: ' + (report.errors.length + report.suppressed_errors) + '.');
    return messages.join(' ');
  }
  root.FocusModel = Object.freeze({ PAGE_SIZE, page, ticks, warnings });
})(globalThis);
