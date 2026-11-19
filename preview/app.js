/* ATLAS: a local-only report workbench. Imported data is never interpreted as markup. */
(function () {
  'use strict';
  const R = globalThis.PruneReport;
  const $ = id => document.getElementById(id);
  const number = value => new Intl.NumberFormat('ru-RU').format(value);
  const date = value => {
    const d = R.dateFromSeconds(value);
    return d ? d.toLocaleDateString('ru-RU') : 'Дата вне диапазона';
  };
  const text = (node, value) => { node.textContent = R.displayText(value); };
  function element(tag, value, className) {
    const node = document.createElement(tag);
    if (value !== undefined) text(node, value);
    if (className) node.className = className;
    return node;
  }
  let report, category = '', selected = null, rows = [], imported = false;
  let importGeneration = 0;
  const PAGE_SIZE = 30;
  let page = 0, initiatingRow = null;
  const riskAdvice = {
    safe: 'Минимальный риск не означает, что удаление обязательно или полностью безопасно.',
    low: 'Файлы могут понадобиться снова. Учитывайте время и условия восстановления.',
    medium: 'Сначала проверьте содержимое: здесь могут быть нужные вам данные.',
    high: 'Будьте особенно осторожны. Потеря этих данных может оказаться необратимой.',
    never: 'Эти данные не предназначены для удаления.'
  };
  for (const [key, label] of Object.entries(R.risks)) {
    const option = element('option', label); option.value = key; $('risk').append(option);
  }
  function sumSizes(findings) {
    return R.sumIntegers(findings.map(f => f.size_bytes), report.schema_version === 2 ? 0n : 0);
  }
  function groups() {
    return Object.entries(R.categories).map(([key, label]) => {
      const items = report.findings.filter(f => f.category === key);
      return { key, label, count: items.length, bytes: sumSizes(items) };
    }).filter(group => group.count).sort((a, b) => R.compareIntegers(b.bytes, a.bytes));
  }
  function chooseCategory(key) {
    const origin = document.activeElement.parentElement?.id;
    category = key;
    page = 0;
    renderNavigation();
    renderRows();
    if (origin === 'categories' || origin === 'ribbon') {
      const replacement = Array.from($(origin).children).find(button => button.dataset.category === key);
      replacement?.focus({ preventScroll: true });
    }
  }
  function renderNavigation() {
    $('categories').replaceChildren();
    const all = [{ key: '', label: 'Все категории', bytes: report.total_bytes, count: report.findings.length }, ...groups()];
    for (const group of all) {
      const button = element('button', undefined, 'category');
      button.type = 'button';
      button.dataset.category = group.key;
      button.setAttribute('aria-pressed', String(category === group.key));
      button.append(element('span', group.label), element('span', R.formatBytes(group.bytes), 'category-size'), element('small', number(group.count) + ' записей'));
      button.addEventListener('click', () => chooseCategory(group.key));
      $('categories').append(button);
    }
    $('ribbon').replaceChildren();
    for (const group of groups()) {
      if (!group.bytes) continue;
      const segment = element('span');
      segment.style.flexGrow = String(R.ratioBasisPoints(group.bytes, report.total_bytes));
      segment.style.flexBasis = '0px';
      segment.setAttribute('aria-hidden', 'true');
      if (group.key === category) segment.className = 'active-share';
      segment.title = group.label + ': ' + R.percentage(group.bytes, report.total_bytes) + '%';
      $('ribbon').append(segment);
    }
    if (!report.total_bytes) $('ribbon').append(element('span', 'Нет объёма для распределения'));
    $('ribbon').setAttribute('aria-label', 'Доли объёма: ' + groups().map(group => group.label + ' ' + R.formatBytes(group.bytes)).join(', ') + '. Иллюстрация; категории доступны в навигации.');
  }
  function renderOverview() {
    const formatted = R.formatBytes(report.total_bytes);
    const separator = formatted.lastIndexOf(' ');
    $('total').replaceChildren(document.createTextNode(formatted.slice(0, separator)), element('span', formatted.slice(separator + 1)));
    text($('total-context'), number(report.total_bytes) + ' Б · ' + number(report.total_files) + ' файлов · ' + report.findings.length + ' записей');
    $('report-state').hidden = report.completeness === 'complete';
    $('report-state').className = 'state warning';
    const stateMessage = {
      complete: '',
      partial: 'Неполный отчёт: объём может быть занижен.',
      cancelled: 'Сканирование отменено. Результаты могут быть неполными.',
      unknown: 'Полнота отчёта неизвестна: нельзя считать результат ни полным, ни неполным.'
    }[report.completeness];
    text($('report-state'), stateMessage);
    const largest = groups()[0];
    text($('dominant-category'), largest && report.total_bytes ? largest.label + ' · ' + R.percentage(largest.bytes, report.total_bytes) + '% объёма' : 'Категории без объёма');
    const completenessNote = {
      complete: '',
      partial: 'Данные неполные; показаны только полученные записи. ',
      cancelled: 'Сканирование отменено; результаты могут быть частичными. ',
      unknown: 'Полнота отчёта неизвестна; отсутствие записей не доказывает, что подходящих файлов нет. '
    }[report.completeness];
    text($('safety-note'), completenessNote + 'Логический размер, не освобождаемое место. Сумма не учитывает совместные блоки, сжатие и снимки.');
    text($('platform'), report.platform);
    text($('scan-date'), date(report.scanned_at));
    const count = report.diagnostic_count;
    $('report-errors').hidden = !count;
    text($('error-summary'), 'Ошибки доступа и чтения: ' + number(count));
    $('error-list').replaceChildren();
    for (const error of report.errors) $('error-list').append(element('li', error.path + ' — ' + error.message));
    if (report.suppressed_errors) $('error-list').append(element('li', 'Ещё ' + number(report.suppressed_errors) + ' ошибок без подробностей.'));
  }
  function renderRows() {
    rows = R.filterFindings(report.findings, { category, risk: $('risk').value, query: $('search').value, sort: $('sort').value });
    if (!rows.some(f => f.index === selected)) selected = rows.length ? rows[0].index : null;
    text($('results-title'), category ? R.categories[category] : 'Все категории');
    text($('result-count'), rows.length + ' из ' + report.findings.length + ' записей · ' + R.formatBytes(sumSizes(rows)));
    $('findings').replaceChildren();
    page = Math.min(page, Math.max(0, Math.ceil(rows.length / PAGE_SIZE) - 1));
    const visible = rows.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
    if (!visible.some(f => f.index === selected)) selected = visible[0]?.index ?? null;
    visible.forEach(finding => {
      const button = element('button', undefined, 'finding'); button.type = 'button';
      button.dataset.index = String(finding.index);
      button.setAttribute('aria-pressed', String(selected === finding.index));
      button.setAttribute('aria-controls', 'inspector');
      const name = element('span');
      name.append(element('span', finding.rule_name, 'row-name'), element('span', finding.path, 'row-path'));
      if (finding.path_encoding === 'lossy_utf8') name.append(element('small', 'Путь преобразован с потерями UTF-8', 'path-lossy'));
      const size = element('span', R.formatBytes(finding.size_bytes), 'row-size');
      size.append(element('small', R.risks[finding.risk] + (finding.partial ? ' · неполно' : '')));
      button.append(name, size);
      button.addEventListener('click', () => {
        selected = finding.index;
        initiatingRow = finding.index;
        for (const row of $('findings').children) row.setAttribute('aria-pressed', String(row === button));
        renderInspector();
      });
      $('findings').append(button);
    });
    $('empty').hidden = rows.length > 0;
    const hasFindings = report.findings.length > 0;
    text($('empty-message'), hasFindings ? 'По этим условиям ничего не найдено. Измените поиск, риск или категорию.' : emptyReportMessage());
    $('clear-filters').hidden = !hasFindings;
    $('pagination').hidden = rows.length <= PAGE_SIZE;
    $('previous-page').disabled = page === 0;
    $('next-page').disabled = (page + 1) * PAGE_SIZE >= rows.length;
    text($('page-label'), 'Страница ' + (page + 1) + ' из ' + Math.max(1, Math.ceil(rows.length / PAGE_SIZE)));
    $('jump-detail').disabled = selected === null;
    renderInspector();
  }
  function renderInspector() {
    const target = $('inspector-content');
    target.replaceChildren();
    const current = selected === null ? null : report.findings[selected];
    text($('selection-status'), current ? 'Выбрано: ' + current.rule_name : 'Запись не выбрана.');
    if (selected === null) {
      const title = element('h2', report.findings.length ? 'Ничего не выбрано' : 'Нет записей для просмотра'); title.id = 'inspector-title';
      target.append(title, element('p', report.findings.length ? 'Измените фильтры, чтобы увидеть запись.' : emptyReportMessage(), 'inspector-description'));
      appendReportContext(target);
      return;
    }
    const f = report.findings[selected];
    const title = element('h2', f.rule_name); title.id = 'inspector-title';
    target.append(title, element('p', R.formatBytes(f.size_bytes), 'inspector-size'), element('p', number(f.size_bytes) + ' Б · ' + number(f.file_count) + ' файлов', 'inspector-count'), element('p', f.path, 'inspector-path'));
    if (f.path_encoding === 'lossy_utf8') target.append(element('p', R.lossyPathWarning, 'path-warning'));
    target.append(element('h3', 'Что это'), element('p', f.description, 'inspector-description'));
    target.append(element('h3', 'Риск · ' + R.risks[f.risk]), element('p', riskAdvice[f.risk], 'inspector-description'));
    const dl = element('dl');
    const pairs = [
      ['Как данные могут появиться снова', R.recreates[f.recreate] + '. Возврат прежнего содержимого не гарантирован.'],
      ['Последнее изменение', f.last_modified === null ? 'Нет данных' : date(f.last_modified)],
      ['Полнота записи', f.partial ? 'Неполная — размер может быть занижен' : 'Нет признака неполноты'],
      ['Правило', f.rule_id]
    ];
    for (const [label, value] of pairs) dl.append(element('dt', label), element('dd', value));
    target.append(dl);
    appendReportContext(target);
  }
  function emptyReportMessage() {
    if (report.completeness === 'cancelled') return 'Сканирование отменено; отсутствие записей не означает, что подходящих файлов нет.';
    if (report.completeness === 'partial') return 'В неполном отчёте нет записей. Это не значит, что файлов нет. Откройте другой отчёт.';
    if (report.completeness === 'unknown') return 'Полнота отчёта неизвестна. Отсутствие записей не подтверждает, что проверка ничего не нашла.';
    return 'В этом отчёте нет записей. Чтобы посмотреть другие данные, откройте другой отчёт.';
  }
  function appendReportContext(target) {
    const context = element('section', undefined, 'report-context');
    context.append(element('h3', 'Контекст отчёта'));
    const dl = element('dl');
    dl.append(element('dt', 'Полнота отчёта'), element('dd', R.completenessLabels[report.completeness] + (report.completeness === 'unknown' ? ' — состояние не подтверждено' : '')));
    context.append(dl);
    if (report.scope) {
      context.append(element('h4', 'Область сканирования'));
      const scope = element('dl');
      for (const [label, value] of R.scopeDetails(report.scope)) scope.append(element('dt', label), element('dd', value));
      context.append(scope);
    } else {
      context.append(element('p', 'Область сканирования и сопоставимость не указаны в отчёте версии 1.', 'inspector-description'));
    }
    target.append(context);
  }
  function resetFilters() {
    category = ''; page = 0; $('search').value = ''; $('risk').value = ''; $('sort').value = 'size';
  }
  function load(next, name) {
    report = next; imported = Boolean(name); selected = null; initiatingRow = null; resetFilters();
    text($('source-label'), imported ? 'Локальный файл · ' + name : 'Вымышленное демо');
    text($('source-note'), imported ? 'Локальный отчёт. Источник и достоверность данных не проверены.' : 'Вымышленные пути и размеры. Это не данные вашего Mac.');
    $('demo-button').hidden = !imported;
    renderOverview(); renderNavigation(); renderRows();
  }
  $('open-button').addEventListener('click', () => $('file-input').click());
  $('file-input').addEventListener('change', async () => {
    const file = $('file-input').files[0];
    if (!file) return;
    const generation = ++importGeneration;
    try {
      if (file.size > R.MAX_BYTES) throw new Error('Размер JSON превышает 5 МиБ.');
      const parsed = R.parseReport(await file.text());
      if (generation !== importGeneration) return;
      load(parsed, file.name);
      text($('import-status'), 'Открыт локальный отчёт. Данные не отправляются в сеть.');
    } catch (error) {
      if (generation !== importGeneration) return;
      text($('import-status'), 'Не удалось открыть отчёт. ' + error.message + ' Предыдущий отчёт сохранён.');
    }
    $('import-status').hidden = false;
    $('file-input').value = '';
  });
  $('demo-button').addEventListener('click', () => {
    importGeneration++;
    load(R.validateReport(globalThis.AtlasDemo));
    $('import-status').hidden = true;
  });
  $('clear-filters').addEventListener('click', () => { resetFilters(); renderNavigation(); renderRows(); $('search').focus(); });
   function filterChanged() {
     const filtered = R.filterFindings(report.findings, { category, risk: $('risk').value, query: $('search').value, sort: $('sort').value });
     page = Math.max(0, Math.floor(filtered.findIndex(f => f.index === selected) / PAGE_SIZE));
     renderRows();
   }
   $('search').addEventListener('input', filterChanged);
   $('risk').addEventListener('change', filterChanged);
   $('sort').addEventListener('change', filterChanged);
   $('previous-page').addEventListener('click', () => { page--; renderRows(); });
   $('next-page').addEventListener('click', () => { page++; renderRows(); });
   $('jump-detail').addEventListener('click', () => {
     initiatingRow = selected;
     $('inspector').focus();
     $('inspector').scrollIntoView({ block: 'nearest' });
   });
   $('back-to-list').addEventListener('click', () => {
     const index = rows.findIndex(f => f.index === initiatingRow);
     if (index < 0) { $('results-title').focus(); return; }
     page = Math.floor(index / PAGE_SIZE);
     renderRows();
     Array.from($('findings').children).find(row => row.dataset.index === String(initiatingRow))?.focus();
   });
  load(R.validateReport(globalThis.AtlasDemo));
})();
