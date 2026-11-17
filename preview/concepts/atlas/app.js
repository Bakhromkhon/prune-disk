/* ATLAS: a local-only report workbench. Imported data is never interpreted as markup. */
(function () {
  'use strict';
  const R = globalThis.PruneReport;
  const $ = id => document.getElementById(id);
  const number = value => new Intl.NumberFormat('ru-RU').format(value);
  const date = value => {
    const d = new Date(value * 1000);
    return Number.isNaN(d.getTime()) ? 'Дата вне диапазона' : d.toLocaleDateString('ru-RU');
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
  function groups() {
    return Object.entries(R.categories).map(([key, label]) => {
      const items = report.findings.filter(f => f.category === key);
      return { key, label, count: items.length, bytes: items.reduce((n, f) => n + f.size_bytes, 0) };
    }).filter(group => group.count).sort((a, b) => b.bytes - a.bytes);
  }
  function chooseCategory(key) {
    const origin = document.activeElement.parentElement?.id;
    category = key;
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
      const share = group.bytes / report.total_bytes;
      const button = element('button');
      button.type = 'button';
      button.dataset.category = group.key;
      button.style.flexGrow = String(group.bytes);
      button.style.flexBasis = '0px';
      button.setAttribute('aria-pressed', String(group.key === category));
      // In the overview, the largest share is the single accent, not a second dataset.
      if (!category && $('ribbon').childElementCount === 0) button.className = 'lead-share';
      button.setAttribute('aria-label', group.label + ': ' + R.formatBytes(group.bytes) + ', ' + Math.round(share * 100) + '% объёма отчёта');
      button.title = button.getAttribute('aria-label');
      if (share > .13) button.append(element('span', Math.round(share * 100) + '%'));
      button.addEventListener('click', () => chooseCategory(group.key));
      $('ribbon').append(button);
    }
    if (!report.total_bytes) $('ribbon').append(element('span', 'Нет объёма для распределения'));
  }
  function renderOverview() {
    const formatted = R.formatBytes(report.total_bytes);
    const separator = formatted.lastIndexOf(' ');
    $('total').replaceChildren(document.createTextNode(formatted.slice(0, separator)), element('span', formatted.slice(separator + 1)));
    text($('total-context'), number(report.total_bytes) + ' Б · ' + number(report.total_files) + ' файлов · ' + report.findings.length + ' записей');
    text($('report-state'), report.cancelled ? 'ОТМЕНЁН · НЕПОЛНЫЙ' : report.partial ? 'НЕПОЛНЫЙ ОТЧЁТ' : 'ПОЛНЫЙ ПО ДАННЫМ JSON');
    const largest = groups()[0];
    text($('dominant-category'), largest && report.total_bytes ? largest.label + ' · ' + Math.round(largest.bytes / report.total_bytes * 100) + '% объёма' : 'Категории без объёма');
    text($('safety-note'), (report.cancelled ? 'Сканирование было отменено. ' : '') + (report.partial ? 'Данные неполные; показаны только полученные записи. ' : '') + 'Логический размер, не освобождаемое место. Сумма не учитывает совместные блоки, сжатие и снимки.');
    text($('platform'), report.platform);
    text($('scan-date'), date(report.scanned_at));
    const count = report.errors.length + report.suppressed_errors;
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
    text($('result-count'), rows.length + ' из ' + report.findings.length + ' записей · ' + R.formatBytes(rows.reduce((n, f) => n + f.size_bytes, 0)));
    $('findings').replaceChildren();
    rows.forEach((finding, position) => {
      const button = element('button', undefined, 'finding'); button.type = 'button';
      button.setAttribute('aria-pressed', String(selected === finding.index));
      button.setAttribute('aria-controls', 'inspector');
      const name = element('span');
      name.append(element('span', finding.rule_name, 'row-name'), element('span', finding.path, 'row-path'));
      const size = element('span', R.formatBytes(finding.size_bytes), 'row-size');
      size.append(element('small', R.risks[finding.risk] + (finding.partial ? ' · неполно' : '')));
      button.append(element('span', String(position + 1).padStart(2, '0'), 'row-number'), name, size);
      button.addEventListener('click', () => {
        selected = finding.index;
        for (const row of $('findings').children) row.setAttribute('aria-pressed', String(row === button));
        renderInspector();
        if (matchMedia('(max-width: 700px)').matches) {
          $('inspector').scrollIntoView({ block: 'start' });
          $('inspector').focus({ preventScroll: true });
        }
      });
      $('findings').append(button);
    });
    $('empty').hidden = rows.length > 0;
    renderInspector();
  }
  function renderInspector() {
    const target = $('inspector-content');
    target.replaceChildren();
    if (selected === null) {
      const title = element('h2', 'Ничего не выбрано'); title.id = 'inspector-title';
      target.append(title, element('p', 'Выберите категорию или измените фильтры, чтобы увидеть запись.', 'inspector-description'));
      return;
    }
    const f = report.findings[selected];
    const title = element('h2', f.rule_name); title.id = 'inspector-title';
    target.append(title, element('p', R.formatBytes(f.size_bytes), 'inspector-size'), element('p', number(f.size_bytes) + ' Б · ' + number(f.file_count) + ' файлов', 'inspector-count'), element('p', f.path, 'inspector-path'));
    target.append(element('h3', 'Что это'), element('p', f.description, 'inspector-description'));
    target.append(element('h3', 'Риск · ' + R.risks[f.risk]), element('p', riskAdvice[f.risk], 'inspector-description'));
    const dl = element('dl');
    const pairs = [
      ['Восстановление', R.recreates[f.recreate]],
      ['Последнее изменение', f.last_modified === null ? 'Нет данных' : date(f.last_modified)],
      ['Полнота записи', f.partial ? 'Неполная — размер может быть занижен' : 'Нет признака неполноты'],
      ['Правило', f.rule_id]
    ];
    for (const [label, value] of pairs) dl.append(element('dt', label), element('dd', value));
    target.append(dl);
  }
  function resetFilters() {
    category = ''; $('search').value = ''; $('risk').value = ''; $('sort').value = 'size';
  }
  function load(next, name) {
    report = next; imported = Boolean(name); selected = null; resetFilters();
    text($('source-label'), imported ? 'Локальный файл · ' + name : 'Вымышленное демо');
    text($('source-note'), imported ? 'Импортированный JSON. Источник и достоверность данных не проверены.' : 'Вымышленные пути и размеры. Это не данные вашего Mac.');
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
  $('search').addEventListener('input', renderRows);
  $('risk').addEventListener('change', renderRows);
  $('sort').addEventListener('change', renderRows);
  load(R.validateReport(globalThis.AtlasDemo));
})();
