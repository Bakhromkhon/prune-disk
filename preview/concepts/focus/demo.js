/* Fictional fixture, same values as preview/demo-report.json; no fetch required. */
(function (root) {
  'use strict';
  root.FocusDemo = {
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
})(globalThis);
