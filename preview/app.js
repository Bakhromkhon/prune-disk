'use strict';
const { categories, risks, recreates, MAX_BYTES, validateReport, parseReport, displayText, formatBytes, filterFindings } = PruneReport;
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
const PAGE_SIZE = 30;
function element(tag, text, className) {
  const el = document.createElement(tag);
  if (text !== undefined) el.textContent = displayText(text);
  if (className) el.className = className;
  return el;
}
function badge(risk) { return element('span', risks[risk], 'risk ' + risk); }
function date(seconds) {
  const d = new Date(seconds * 1000);
  return Number.isNaN(d.getTime()) ? 'Дата вне диапазона' : new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium', timeZone: 'UTC' }).format(d) + ' · UTC';
}
function resetFilters() {
  $('category').value = ''; $('risk').value = ''; $('search-input').value = '';
  $('sort').value = 'size'; page = 0; selected = 0;
}
function showReport(value, demo, filename) {
  report = validateReport(value);
  resetFilters();
  put('source-badge', demo ? 'ДЕМО' : 'ВАШ JSON');
  put('source-name', demo ? 'Вымышленный отчёт · не данные вашего устройства' : filename);
  put('report-meta', report.platform + ' · ' + date(report.scanned_at) + ' · схема v1');
  put('total-bytes', formatBytes(report.total_bytes));
  put('total-findings', numberText(report.findings.length));
  put('total-files', numberText(report.total_files) + ' файлов в отчёте');
  put('attention', numberText(report.findings.filter(f => ['medium', 'high', 'never'].includes(f.risk)).length));
  $('warning').hidden = !report.partial;
  put('warning', (report.cancelled ? 'Сканирование отменено. ' : '') + 'Отчёт неполный: объёмы могут быть занижены. Ошибок: ' + report.errors.length + '; подавлено ошибок: ' + numberText(report.suppressed_errors) + '.');
  renderCategories(); renderRows(); renderDiagnostics();
}
function renderCategories() {
  const container = document.querySelector('.category-bars') || element('div', undefined, 'category-bars');
  container.replaceChildren();
  for (const [key, title] of Object.entries(categories)) {
    const sum = report.findings.filter(f => f.category === key).reduce((n, f) => n + f.size_bytes, 0);
    if (!report.findings.some(f => f.category === key)) continue;
    const item = element('div'), label = element('div', undefined, 'category-label');
    label.append(element('strong', title), element('span', formatBytes(sum)));
    const meter = element('meter'); meter.min = 0; meter.max = report.total_bytes || 1; meter.value = sum;
    meter.setAttribute('aria-label', title + ': ' + formatBytes(sum) + ' из ' + formatBytes(report.total_bytes));
    item.append(label, meter); container.append(item);
  }
  if (!container.childElementCount) container.append(element('p', 'В отчёте нет расположений.'));
  document.querySelector('.breakdown').append(container);
}
function renderRows() {
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
    if (f.partial) name.append(element('span', 'Неполное измерение', 'risk medium'));
    const risk = element('td'); risk.append(badge(f.risk));
    row.append(name, risk, element('td', formatBytes(f.size_bytes), 'numeric')); fragment.append(row);
  }
  $('rows').replaceChildren(fragment);
  put('result-count', numberText(filtered.length));
  $('empty').hidden = filtered.length > 0;
  put('shown', 'Показано ' + Math.min(PAGE_SIZE, Math.max(0, filtered.length - page * PAGE_SIZE)) + ' из ' + numberText(filtered.length));
  put('page-label', (page + 1) + ' / ' + Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)));
  $('previous').disabled = page === 0; $('next').disabled = (page + 1) * PAGE_SIZE >= filtered.length;
  renderDetail();
}
function renderDetail() {
  const f = report.findings[selected], container = $('detail-content');
  container.replaceChildren();
  put('detail-summary', f ? 'Выбрано: ' + f.rule_name + '. Риск: ' + risks[f.risk] + '. Объём: ' + formatBytes(f.size_bytes) + '.' : 'Расположение не выбрано.');
  if (!f) { container.append(element('p', 'Выберите расположение в таблице.', 'detail-description')); return; }
  const heading = element('h3', f.rule_name, 'detail-name');
  const dl = element('dl');
  for (const [key, value] of [
    ['Логический объём', formatBytes(f.size_bytes)], ['Файлов', numberText(f.file_count)],
    ['Категория', categories[f.category]], ['Восстановление', recreates[f.recreate]],
    ['Изменено', f.last_modified === null ? 'Нет данных' : date(f.last_modified)],
    ['Измерение', f.partial ? 'Неполное' : 'Без отметки неполноты']
  ]) { const pair = element('div'); pair.append(element('dt', key), element('dd', value)); dl.append(pair); }
  container.append(heading, badge(f.risk), element('div', f.path, 'detail-path'), element('p', f.description, 'detail-description'), dl,
    element('p', 'Только просмотр. Никакие файлы не изменяются. Перед любыми действиями проверьте содержимое и возможность восстановления.', 'detail-note'));
}
function renderDiagnostics() {
  put('diagnostic-summary', 'Диагностика · ' + report.errors.length + ' ошибок · подавлено: ' + numberText(report.suppressed_errors));
  put('diagnostic-note', report.partial ? 'Есть неполные измерения. Подавленные ошибки не содержатся в файле отчёта.' : 'Отчёт не содержит отметки о неполноте. Это не гарантия безопасности удаления.');
  $('error-list').replaceChildren(...report.errors.map(e => element('li', e.path + ' — ' + e.message)));
}
for (const [key, value] of Object.entries(categories)) $('category').append(new Option(value, key));
for (const [key, value] of Object.entries(risks)) $('risk').append(new Option(value, key));
for (const id of ['category', 'risk', 'sort', 'search-input']) $(id).addEventListener(id === 'search-input' ? 'input' : 'change', () => { page = 0; renderRows(); });
$('previous').addEventListener('click', () => { page--; renderRows(); });
$('next').addEventListener('click', () => { page++; renderRows(); });
$('demo-button').addEventListener('click', () => { importSequence++; showReport(DEMO_REPORT, true); put('import-status', 'Показаны вымышленные демонстрационные данные.'); $('import-status').classList.remove('error'); });
$('import-button').addEventListener('click', () => $('file-input').click());
$('file-input').addEventListener('change', () => {
  const file = $('file-input').files[0], sequence = ++importSequence;
  $('file-input').value = '';
  if (!file) return;
  function error(message) { if (sequence !== importSequence) return; put('import-status', message + ' Текущий отчёт сохранён.'); $('import-status').classList.add('error'); }
  if (file.size > MAX_BYTES) { error('Файл больше 5 МиБ.'); return; }
  const reader = new FileReader();
  reader.onerror = () => error('Не удалось прочитать файл.');
  reader.onload = () => {
    if (sequence !== importSequence) return;
    try {
      const imported = parseReport(reader.result);
      showReport(imported, false, file.name);
      $('import-status').classList.remove('error');
      put('import-status', 'Отчёт импортирован локально. Данные никуда не отправлялись.');
    } catch (e) { error(e.message); }
  };
  reader.readAsText(file, 'UTF-8');
});
showReport(DEMO_REPORT, true);
