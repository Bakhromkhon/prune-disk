'use strict';
const { categories, risks, recreates, MAX_BYTES, validateReport, parseReport, displayText, formatBytes, sumIntegers, compareIntegers, ratioBasisPoints, percentage, dateFromSeconds, scopeDetails, filterFindings } = PruneReport;
// Deliberately fabricated, portable demo. No paths or data from the host machine.
const DEMO_REPORT = {
  schema_version: 1, platform: 'macos', scanned_at: 1794902400,
  total_bytes: 17490247680, total_files: 68240, byte_accounting: 'logical',
  partial: false, cancelled: false, suppressed_errors: 0, errors: [],
  findings: [
    { rule_id: 'demo-build', rule_name: 'Артефакты сборки', category: 'developer', risk: 'low', recreate: 'rebuild', path: '/example/projects/sample/target', size_bytes: 7516192768, file_count: 31200, last_modified: 1794816000, description: 'Промежуточные результаты сборки демонстрационного проекта. Следующая сборка может занять больше времени.' },
    { rule_id: 'demo-packages', rule_name: 'Кэш пакетов', category: 'developer', risk: 'safe', recreate: 'download', path: '/example/home/Library/Caches/package-manager', size_bytes: 3758096384, file_count: 22500, last_modified: 1794729600, description: 'Локальные копии загруженных пакетов. Для восстановления потребуется подключение к сети.' },
    { rule_id: 'demo-browser', rule_name: 'Кэш браузера', category: 'browser', risk: 'low', recreate: 'automatic', path: '/example/home/Library/Caches/SampleBrowser', size_bytes: 2362232012, file_count: 8400, last_modified: 1794816000, description: 'Временные ресурсы веб-страниц. Это не история, не пароли и не пользовательский профиль.' },
    { rule_id: 'demo-trash', rule_name: 'Содержимое корзины', category: 'trash', risk: 'medium', recreate: 'manual', path: '/example/home/.Trash', size_bytes: 1825361101, file_count: 640, last_modified: 1794643200, description: 'В корзине могут оставаться нужные файлы. Состав необходимо проверить вручную.' },
    { rule_id: 'demo-apps', rule_name: 'Кэш приложений', category: 'apps', risk: 'medium', recreate: 'unknown', path: '/example/home/Library/Caches/SampleEditor', size_bytes: 1073741824, file_count: 4300, last_modified: 1794556800, description: 'Назначение отдельных файлов зависит от приложения. Восстановление не гарантировано.' },
    { rule_id: 'demo-system', rule_name: 'Диагностические журналы', category: 'system', risk: 'high', recreate: 'impossible', path: '/example/home/Library/Logs/SampleService', size_bytes: 954623591, file_count: 1200, last_modified: 1794470400, description: 'Журналы могут потребоваться для расследования ошибок. Исторические записи не восстанавливаются.' }
  ]
};
const $ = id => document.getElementById(id);
const numberText = n => new Intl.NumberFormat('ru-RU').format(n);
const put = (id, text) => { $(id).textContent = displayText(text); };
let report, selected = 0, page = 0, filtered = [], importSequence = 0;
let detailTrigger = null, announcementSequence = 0;
const PAGE_SIZE = 30;
function element(tag, text, className) {
  const el = document.createElement(tag);
  if (text !== undefined) el.textContent = displayText(text);
  if (className) el.className = className;
  return el;
}
function badge(risk) { return element('span', risks[risk], 'risk ' + risk); }
function date(seconds) {
  const d = dateFromSeconds(seconds);
  return !d ? 'Дата вне диапазона' : new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium', timeZone: 'UTC' }).format(d) + ' · UTC';
}
function sumSizes(findings) {
  return sumIntegers(findings.map(f => f.size_bytes), report.schema_version === 2 ? 0n : 0);
}
function resetFilters() {
  $('category').value = ''; $('risk').value = ''; $('search-input').value = '';
  $('sort').value = 'size'; page = 0; selected = 0;
}
function showReport(value, demo, filename) {
  report = validateReport(value);
  resetFilters();
  put('source-badge', demo ? 'Пример — не данные компьютера' : 'Открытый файл');
  put('source-name', demo ? 'Вымышленный отчёт' : filename);
  put('report-meta', report.platform + ' · ' + date(report.scanned_at));
  put('total-bytes', formatBytes(report.total_bytes));
  put('total-findings', numberText(report.findings.length));
  put('total-files', numberText(report.total_files) + ' файлов в отчёте');
  put('attention', numberText(report.findings.filter(f => ['medium', 'high', 'never'].includes(f.risk)).length));
  const warningText = {
    complete: '',
    partial: 'Отчёт неполный: размеры могут быть занижены.',
    cancelled: 'Сканирование отменено: результаты могут быть неполными.',
    unknown: 'Полнота отчёта неизвестна по данным отчёта.'
  }[report.completeness];
  $('warning').hidden = report.completeness === 'complete';
  put('warning', 'По данным отчёта: ' + warningText + ' Ошибок с описанием: ' + report.errors.length + '. Ещё ' + numberText(report.suppressed_errors) + ' ошибок без описания; подробности — в диагностике.');
  renderCategories(); renderRows(); renderDiagnostics();
}
function renderCategories() {
  const container = document.querySelector('.category-bars') || element('div', undefined, 'category-bars');
  container.replaceChildren();
  const totals = Object.entries(categories).map(([key, title]) => ({
    key, title, sum: sumSizes(report.findings.filter(f => f.category === key))
  })).filter(({ key }) => report.findings.some(f => f.category === key)).sort((a, b) => compareIntegers(b.sum, a.sum));
  for (const { title, sum } of totals) {
    const percent = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 1 }).format(percentage(sum, report.total_bytes, 1)) + '%';
    const item = element('div'), label = element('div', undefined, 'category-label');
    label.append(element('strong', title), element('span', formatBytes(sum) + ' · ' + percent));
    const meter = element('meter'); meter.min = 0; meter.max = 10000; meter.value = ratioBasisPoints(sum, report.total_bytes);
    meter.setAttribute('aria-label', title + ': ' + percent + ', ' + formatBytes(sum) + ' из ' + formatBytes(report.total_bytes));
    item.append(label, meter); container.append(item);
  }
  if (!container.childElementCount) container.append(element('p', 'В отчёте нет расположений.'));
  document.querySelector('.breakdown').append(container);
}
function renderRows() {
  detailTrigger = null;
  filtered = filterFindings(report.findings, { category: $('category').value, risk: $('risk').value, query: $('search-input').value, sort: $('sort').value });
  page = Math.min(page, Math.max(0, Math.ceil(filtered.length / PAGE_SIZE) - 1));
  const visible = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  if (!visible.some(f => f.index === selected)) selected = visible[0]?.index ?? -1;
  const fragment = document.createDocumentFragment();
  for (const f of visible) {
    const row = element('tr');
    if (f.index === selected) row.className = 'selected';
    const name = element('td'), button = element('button', f.rule_name, 'row-button');
    button.type = 'button'; button.setAttribute('aria-pressed', String(f.index === selected));
    button.setAttribute('aria-controls', 'detail-panel');
    button.setAttribute('aria-label', 'Подробности: ' + displayText(f.rule_name));
    button.addEventListener('click', () => {
      detailTrigger = button;
      selected = f.index;
      // Keep keyboard focus on the original button, rather than replacing the table.
      for (const sibling of $('rows').children) {
        const active = sibling === row; sibling.classList.toggle('selected', active);
        sibling.querySelector('button').setAttribute('aria-pressed', String(active));
      }
      renderDetail();
    });
    const path = element('span', f.path, 'path'); path.title = displayText(f.path);
    name.append(button, path);
    if (f.path_encoding === 'lossy_utf8') name.append(element('span', 'Путь преобразован с потерями UTF-8', 'path-lossy'));
    if (f.partial) name.append(element('span', 'Неполное измерение', 'risk medium'));
    const jump = element('button', 'Перейти к подробностям', 'row-jump');
    jump.type = 'button';
    jump.setAttribute('aria-controls', 'detail-panel');
    jump.setAttribute('aria-label', 'Перейти к подробностям: ' + displayText(f.rule_name));
    jump.addEventListener('click', () => {
      button.click();
      detailTrigger = jump;
      $('detail-title').focus();
    });
    name.append(jump);
    const risk = element('td'); risk.append(badge(f.risk));
    row.append(name, risk, element('td', formatBytes(f.size_bytes), 'numeric')); fragment.append(row);
  }
  $('rows').replaceChildren(fragment);
  put('result-count', numberText(filtered.length));
  $('empty').hidden = filtered.length > 0;
  const emptyMessage = report.completeness === 'partial' ?
    'В неполном отчёте нет расположений. Это не значит, что проверка ничего не нашла: откройте диагностику и получите новый отчёт.' :
    report.completeness === 'cancelled' ?
      'Сканирование отменено; отсутствие расположений не подтверждает, что проверка ничего не нашла.' :
      report.completeness === 'unknown' ?
        'Полнота отчёта неизвестна. Отсутствие расположений не подтверждает, что проверка ничего не нашла.' :
        'В отчёте нет расположений. Это не оценка всего компьютера: просмотрщик показывает только содержимое файла.';
  put('empty', report.findings.length ? 'Нет совпадений с поиском и фильтрами. Измените условия или сбросьте фильтры.' : emptyMessage);
  $('reset-filters').hidden = !($('category').value || $('risk').value || $('search-input').value || $('sort').value !== 'size');
  $('jump-detail').disabled = selected < 0;
  $('back-to-row').disabled = selected < 0;
  put('shown', 'Показано ' + Math.min(PAGE_SIZE, Math.max(0, filtered.length - page * PAGE_SIZE)) + ' из ' + numberText(filtered.length));
  put('page-label', 'Страница ' + (page + 1) + ' из ' + Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)));
  $('previous').disabled = page === 0; $('next').disabled = (page + 1) * PAGE_SIZE >= filtered.length;
  renderDetail();
}
function renderDetail() {
  const f = report.findings[selected], container = $('detail-content');
  container.replaceChildren();
  put('detail-summary', f ? 'Выбрано: ' + f.rule_name + '. Риск: ' + risks[f.risk] + '. Объём: ' + formatBytes(f.size_bytes) + '.' : 'Расположение не выбрано.');
  if (!f) { container.append(element('p', report.findings.length ? 'Нет подходящих расположений. Сбросьте фильтры, чтобы вернуть список.' : 'В этом отчёте нет расположений для просмотра.', 'detail-description')); return; }
  const heading = element('h3', f.rule_name, 'detail-name');
  const dl = element('dl');
  for (const [key, value] of [
    ['Логический объём', formatBytes(f.size_bytes)], ['Файлов', numberText(f.file_count)],
    ['Категория', categories[f.category]], ['Как данные могут появиться снова', recreates[f.recreate]],
    ['Изменено', f.last_modified === null ? 'Нет данных' : date(f.last_modified)],
    ['Измерение', f.partial ? 'Неполное' : 'Без отметки неполноты']
  ]) { const pair = element('div'); pair.append(element('dt', key), element('dd', value)); dl.append(pair); }
  container.append(heading, badge(f.risk), element('div', f.path, 'detail-path'));
  if (f.path_encoding === 'lossy_utf8') container.append(element('p', PruneReport.lossyPathWarning, 'path-warning'));
  container.append(element('p', f.description, 'detail-description'), dl,
    element('p', 'Это не гарантия восстановления удалённых файлов. Только просмотр: никакие файлы не изменяются. Перед любыми действиями проверьте содержимое и резервные копии.', 'detail-note'));
}
function renderDiagnostics() {
  put('diagnostic-summary', 'Диагностика · всего: ' + numberText(report.diagnostic_count) + ' · описано: ' + report.errors.length + ' · подавлено: ' + numberText(report.suppressed_errors));
  const completenessNote = {
    complete: 'Отчёт помечен полным по данным отчёта.',
    partial: 'Отчёт помечен как неполный.',
    cancelled: 'Сканирование в отчёте отменено.',
    unknown: 'Полнота отчёта неизвестна.'
  }[report.completeness];
  const suppressedNote = report.suppressed_errors ?
    ' Для ' + numberText(report.suppressed_errors) + ' ошибок описания не записаны в файл отчёта.' : '';
  const scopeNote = report.schema_version === 1 ? ' Область сканирования и сопоставимость не указаны.' : '';
  put('diagnostic-note', 'Версия схемы: ' + report.schema_version + '. ' + completenessNote + scopeNote + suppressedNote + ' Это не гарантия безопасности удаления.');
  const scopeHeading = $('scope-heading'), scopeList = $('scan-scope');
  scopeList.replaceChildren();
  scopeHeading.hidden = !report.scope;
  scopeList.hidden = !report.scope;
  if (report.scope) {
    for (const [label, value] of scopeDetails(report.scope)) {
      scopeList.append(element('dt', label), element('dd', value));
    }
  }
  $('error-list').replaceChildren(...report.errors.map(e => element('li', e.path + ' — ' + e.message)));
}
for (const [key, value] of Object.entries(categories)) $('category').append(new Option(value, key));
for (const [key, value] of Object.entries(risks)) $('risk').append(new Option(value, key));
for (const id of ['category', 'risk', 'sort', 'search-input']) $(id).addEventListener(id === 'search-input' ? 'input' : 'change', () => { page = 0; renderRows(); });
function changePage(delta, trigger) {
  page += delta; renderRows();
  if (trigger.disabled) $('page-label').focus();
}
function selectedButton() {
  return [...$('rows').children].map(row => row.querySelector('button')).find(button => button.getAttribute('aria-pressed') === 'true');
}
$('previous').addEventListener('click', () => changePage(-1, $('previous')));
$('next').addEventListener('click', () => changePage(1, $('next')));
$('reset-filters').addEventListener('click', () => { resetFilters(); renderRows(); $('search-input').focus(); });
$('jump-detail').addEventListener('click', () => { detailTrigger = $('jump-detail'); $('detail-title').focus(); });
$('back-to-row').addEventListener('click', () => (detailTrigger || selectedButton() || $('search-input')).focus());
function announceImport(message, error = false) {
  const sequence = ++announcementSequence;
  put('import-status', '');
  $('import-status').classList.toggle('error', error);
  // Separate clearing and filling across frames so identical failures can be announced again.
  requestAnimationFrame(() => requestAnimationFrame(() => {
    if (sequence === announcementSequence) put('import-status', message);
  }));
}
$('demo-button').addEventListener('click', () => { importSequence++; showReport(DEMO_REPORT, true); $('import-error-detail').hidden = true; announceImport('Показаны вымышленные демонстрационные данные.'); });
$('import-button').addEventListener('click', () => $('file-input').click());
$('file-input').addEventListener('change', () => {
  const file = $('file-input').files[0], sequence = ++importSequence;
  $('file-input').value = '';
  if (!file) return;
  function error(message, action = 'Выберите JSON-отчёт Prune версии 1 или 2 либо получите новый отчёт через CLI.') {
    if (sequence !== importSequence) return;
    put('import-error-message', message);
    $('import-error-detail').hidden = false;
    announceImport('Не удалось открыть отчёт. ' + action + ' Текущий отчёт сохранён.', true);
  }
  if (file.size > MAX_BYTES) { error('Файл больше 5 МиБ.', 'Выберите файл не больше 5 МиБ.'); return; }
  const reader = new FileReader();
  reader.onerror = () => error('Не удалось прочитать файл.', 'Проверьте доступ к файлу и откройте его ещё раз.');
  reader.onload = () => {
    if (sequence !== importSequence) return;
    try {
      const imported = parseReport(reader.result);
      showReport(imported, false, file.name);
      $('import-error-detail').hidden = true;
      announceImport('Отчёт открыт локально. Данные никуда не отправлялись.');
    } catch (e) { error(e.message); }
  };
  reader.readAsText(file, 'UTF-8');
});
showReport(DEMO_REPORT, true);
