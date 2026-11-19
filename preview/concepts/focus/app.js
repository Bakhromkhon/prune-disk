(function () {
  'use strict';
  const R = globalThis.PruneReport, M = globalThis.FocusModel;
  const $ = id => document.getElementById(id);
  const number = value => new Intl.NumberFormat('ru-RU').format(value);
  const date = value => {
    if (value === null) return 'Не указано';
    const d = R.dateFromSeconds(value);
    return d ? d.toLocaleDateString('ru-RU', { timeZone: 'UTC' }) : 'Дата вне диапазона';
  };
  function node(tag, className, text) {
    const el = document.createElement(tag);
    if (className) el.className = className;
    if (text !== undefined) el.textContent = R.displayText(text);
    return el;
  }
  let report, source, selected = 0, current = 0, importVersion = 0;
  const options = { category: '', query: '', sort: 'size' };
  const recreation = {
    automatic: 'По классификации отчёта, данные создаются приложением автоматически. Это не возврат исходных файлов.',
    download: 'Потребуется повторная загрузка. Нужны доступ к источнику и подключение к сети.',
    rebuild: 'Потребуется повторная сборка проекта. Это займёт время и потребует исходников и инструментов.',
    manual: 'Повторное создание — вручную. Наличие оригиналов или резервной копии отчёт не подтверждает.',
    impossible: 'Отчёт считает повторное создание невозможным. Исторические данные могут быть утрачены.',
    unknown: 'Способ повторного создания неизвестен. Не стоит считать эти файлы заменимыми.'
  };
  function renderCategories() {
    const group = $('categories');
    group.replaceChildren();
    const available = new Set(report.findings.map(f => f.category));
    for (const [key, name] of [['', 'Все'], ...Object.entries(R.categories).filter(([key]) => available.has(key))]) {
      const button = node('button', '', name);
      button.type = 'button';
      button.setAttribute('aria-pressed', String(options.category === key));
      button.addEventListener('click', () => {
        options.category = key; current = 0;
        for (const sibling of group.children) sibling.setAttribute('aria-pressed', String(sibling === button));
        renderList();
      });
      group.append(button);
    }
  }
  function appendReportContext(detail) {
    const context = node('section', 'report-context');
    context.append(node('h3', '', 'Контекст отчёта'));
    const facts = node('dl', 'facts');
    const completeness = R.completenessLabels[report.completeness] +
      (report.completeness === 'unknown' ? ' — состояние не подтверждено' : '');
    const row = node('div');
    row.append(node('dt', '', 'Полнота отчёта'), node('dd', '', completeness));
    facts.append(row);
    if (report.scope) {
      context.append(node('h4', '', 'Область сканирования'));
      for (const [label, value] of R.scopeDetails(report.scope)) {
        const item = node('div');
        item.append(node('dt', '', label), node('dd', '', value));
        facts.append(item);
      }
    } else {
      context.append(node('p', 'scope-note', 'Область сканирования и сопоставимость не указаны в отчёте версии 1.'));
    }
    context.append(facts);
    detail.append(context);
  }
  function renderDetail() {
    const detail = $('detail'), f = report.findings[selected];
    detail.replaceChildren();
    $('context-index').textContent = f ? String(selected + 1).padStart(2, '0') + ' / ' + String(report.findings.length).padStart(2, '0') : '—';
    appendReportContext(detail);
    if (!f) {
      detail.append(node('p', 'empty', 'Выберите место в списке, чтобы прочитать его контекст.'));
      return;
    }
    detail.append(node('p', 'detail-category', R.categories[f.category]), node('h3', 'detail-title', f.rule_name), node('p', 'detail-description', f.description || 'В отчёте нет описания этого места.'));
    detail.append(node('span', 'path-label', 'ПУТЬ ИЗ ОТЧЁТА'), node('code', 'path', f.path));
    if (f.path_encoding === 'lossy_utf8') detail.append(node('p', 'path-warning', R.lossyPathWarning));
    const dl = node('dl', 'facts');
    for (const [name, value] of [
      ['Логический объём', R.formatBytes(f.size_bytes)],
      ['Файлов', number(f.file_count)],
      ['Изменено (UTC)', date(f.last_modified)],
      ['Риск по отчёту', R.risks[f.risk]]
    ]) {
      const row = node('div');
      row.append(node('dt', '', name), node('dd', '', value)); dl.append(row);
    }
    detail.append(dl);
    if (f.partial) detail.append(node('p', 'detail-warning', 'Это место измерено частично. Объём и количество файлов могут быть неполными.'));
    const recreate = node('section', 'recreate');
    recreate.append(node('h4', '', 'Можно ли создать заново?'), node('p', '', recreation[f.recreate]));
    detail.append(recreate, node('p', 'context-note', 'Повторное создание — не отмена удаления. Этот просмотрщик ничего не меняет на диске.'));
  }
  function renderList() {
    const page = M.page(report.findings, options, current);
    current = page.current;
    if (!page.items.some(f => f.index === selected)) selected = page.items.length ? page.items[0].index : -1;
    const list = $('list');
    list.replaceChildren();
    $('result-count').textContent = number(page.matches.length) + ' из ' + number(report.findings.length);
    const zero = report.schema_version === 2 ? 0n : 0;
    const maximum = report.findings.reduce((largest, f) =>
      R.compareIntegers(f.size_bytes, largest) > 0 ? f.size_bytes : largest, zero);
    for (const f of page.items) {
      const button = node('button', 'location');
      button.type = 'button';
      button.setAttribute('aria-pressed', String(f.index === selected));
      button.setAttribute('aria-controls', 'context');
      button.setAttribute('aria-label', R.displayText(f.rule_name) + ', ' + R.formatBytes(f.size_bytes) + '. ' +
        (f.path_encoding === 'lossy_utf8' ? 'Путь отображён с потерями UTF-8. ' : '') + 'Показать контекст');
      const label = node('span', 'location-label');
      label.append(node('span', 'location-title', f.rule_name), node('span', 'location-category', R.categories[f.category] + (f.partial ? ' · Неполное измерение' : '')));
      if (f.path_encoding === 'lossy_utf8') label.append(node('span', 'path-lossy', 'Путь преобразован с потерями UTF-8'));
      label.append(node('span', 'mobile-jump', 'Контекст ↓'));
      const size = node('span', 'location-size');
      size.append(node('span', '', R.formatBytes(f.size_bytes)));
      const ticks = node('span', 'ticks');
      ticks.setAttribute('aria-hidden', 'true');
      const count = M.ticks(f.size_bytes, maximum);
      for (let i = 0; i < 24; i++) ticks.append(node('i', 'tick' + (i < count ? ' on' : '')));
      size.append(ticks);
      button.append(node('span', 'ordinal', String(f.index + 1).padStart(2, '0')), label, size);
      button.addEventListener('click', () => {
        selected = f.index;
        for (const sibling of list.children) sibling.setAttribute('aria-pressed', String(sibling === button));
        renderDetail();
        if (window.matchMedia('(max-width: 700px)').matches) {
          $('context').focus({ preventScroll: true });
          $('context').scrollIntoView({ block: 'start', behavior: 'instant' });
        }
      });
      list.append(button);
    }
    if (!page.items.length) list.append(node('p', 'empty', 'Ничего не найдено. Попробуйте другую категорию или измените поиск.'));
    $('page-label').textContent = page.matches.length ? (current * M.PAGE_SIZE + 1) + '–' + Math.min((current + 1) * M.PAGE_SIZE, page.matches.length) + ' из ' + number(page.matches.length) : '0 мест';
    $('prev').disabled = current === 0;
    $('next').disabled = current >= page.pages - 1;
    renderDetail();
  }
  function load(value, name, demo) {
    report = R.validateReport(value);
    source = name; selected = 0; current = 0;
    options.category = ''; options.query = ''; options.sort = 'size';
    $('search').value = ''; $('sort').value = 'size';
    $('source').textContent = demo ? 'ДЕМО · ВЫМЫШЛЕННЫЕ ДАННЫЕ' : 'ЛОКАЛЬНЫЙ ФАЙЛ · ' + R.displayText(source);
    const summary = $('summary');
    summary.replaceChildren(node('strong', '', R.formatBytes(report.total_bytes)), document.createTextNode(' в ' + number(report.findings.length) + ' местах · ' + number(report.total_files) + ' файлов. Это логический объём, не оценка освобождения.'));
    $('report-meta').textContent = 'Отчёт от ' + date(report.scanned_at) + ' (UTC) · ' + R.displayText(report.platform) + (demo ? ' · Не данные вашего устройства' : ' · Снимок, не текущее состояние диска');
    const warning = M.warnings(report);
    $('warning').textContent = warning; $('warning').hidden = !warning;
    renderCategories(); renderList();
  }
  $('search').addEventListener('input', event => { options.query = event.target.value; current = 0; renderList(); });
  $('sort').addEventListener('change', event => { options.sort = event.target.value; current = 0; renderList(); });
  $('prev').addEventListener('click', () => { current--; renderList(); });
  $('next').addEventListener('click', () => { current++; renderList(); });
  $('import-button').addEventListener('click', () => $('file-input').click());
  $('demo-button').addEventListener('click', () => { importVersion++; load(globalThis.FocusDemo, 'Демо', true); $('import-status').textContent = ''; });
  $('file-input').addEventListener('change', async event => {
    const file = event.target.files[0];
    event.target.value = '';
    if (!file) return;
    const version = ++importVersion;
    try {
      if (file.size > R.MAX_BYTES) throw new Error('Размер JSON превышает 5 МиБ.');
      $('import-status').textContent = 'Читаем локальный файл…';
      const text = await file.text();
      if (version !== importVersion) return;
      const validated = R.parseReport(text);
      load(validated, file.name, false);
      $('import-status').textContent = 'Файл открыт локально. Ничего не отправлено в сеть.';
    } catch (error) {
      if (version === importVersion) $('import-status').textContent = 'Не удалось открыть файл. ' + R.displayText(error.message) + ' Предыдущий отчёт сохранён.';
    }
  });
  load(globalThis.FocusDemo, 'Демо', true);
})();
