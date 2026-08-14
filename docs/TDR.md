# TDR — looq

**Status:** Draft v1.0
**Date:** 2026-08-08
**Author:** looq maintainers
**Licence:** MIT

---

## 1. Summary

looq — single-binary CLI на Rust, запускающий локальный web-сервер для просмотра логов из файла или stdin. Парсинг выполняется в WebAssembly на стороне браузера, что обеспечивает полную privacy: содержимое логов не покидает машину пользователя. CLI проксирует только stdin через WebSocket для live-tail.

Целевая аудитория: разработчики и SRE, разбирающие инциденты по `kubectl logs > file.log`, `docker logs > file.log`, или tail'ящие приложение в реальном времени через pipe.

## 2. Goals / Non-Goals

### Goals
- Single binary, zero external dependencies, runs as `looq file.log` или `app | looq`
- Privacy-first: парсинг в WASM на клиенте, файл загружается через File API
- Live tail из stdin через WebSocket
- Auto-detect формата: JSON, logfmt, syslog, plain text с regex
- Time-range filtering через drag по timeline
- Field-based filtering (level=ERROR, service=api)
- Полнотекстовый поиск с подсветкой
- Работа в браузере, Light/Dark темы

### Non-Goals (MVP)
- gzip/zstd decompression (P2)
- Trace correlation / OpenTelemetry parsing (P2)
- Multi-file merge по timestamp (P3)
- Встроенные SQL-запросы (P3)
- SSH remote tail (P3)
- TUI mode (только web)
- Configuration files / пользовательские настройки (P3)
- **MCP server mode** (P2) — см. § 17

## 3. Architecture

```
┌─────────────────────────────────────────┐
│  CLI (Rust native, axum + tokio)        │
│  ├── HTTP server: 127.0.0.1:PORT        │
│  ├── WebSocket: /ws → stdin pipe        │
│  ├── Static: index.html + app.ts        │
│  └── Embedded: core.wasm                │
└─────────────────────────────────────────┘
              ↓ fetch /ws
┌─────────────────────────────────────────┐
│  Browser (frontend, TypeScript)         │
│  ├── File API: file остаётся в JS       │
│  ├── WebSocket: stdin → live tail       │
│  ├── WASM core: парсинг, индекс         │
│  └── UI: timeline, таблица, фильтры     │
└─────────────────────────────────────────┘
```

**Backend выполняет:**
- Приём HTTP-запросов, отдача статики (HTML/JS/WASM, embedded через `include_bytes!`)
- WebSocket-мост между stdin и клиентом
- Чтение argv, настройка порта, graceful shutdown

**Frontend (WASM + TypeScript) выполняет:**
- Парсинг формата (auto-detect по первым 100 строкам)
- Извлечение timestamp + level + произвольных полей
- Построение индекса по timestamp
- Выполнение фильтров и поиска
- Рендер UI (timeline, таблица, virtual scroll)

## 4. Tech Stack

### Backend
- **Rust 1.74+** (edition 2021)
- `axum` 0.8 — HTTP-сервер + WebSocket
- `tokio` 1.x — async runtime
- `serde_json` 1.x — argv parsing
- `regex` 1.x — argv validation
- `chrono` 0.4 — fallback timestamp parsing
- `clap` 4.x — CLI args (опционально)
- `tracing` + `tracing-subscriber` — логи CLI

### Frontend (WASM core)
- **Rust 1.74+** → `wasm-pack` → `core.wasm`
- `wasm-bindgen` 0.2 — JS interop
- `serde-wasm-bindgen` — типобезопасный обмен
- `regex` 1.x — формат-детекторы
- `chrono` 0.4 — парсинг таймстампов
- `web-sys`, `js-sys` — DOM/JS bindings

### Frontend (TypeScript layer)
- **TypeScript 5.x** (strict mode)
- Vite 5.x — bundler, dev server
- Web Components (custom elements, без React/Vue)
- `uPlot` 1.6 — графики timeline (~40 КБ)
- `comlink` 4.x — удобный мост к WASM
- Bundler output: < 200 КБ gzipped

### CDP / Tests
- `wasm-pack test` — unit tests WASM core (node + browser)
- Playwright (опционально, P3) — e2e тесты UI
- `cargo test` — backend unit tests

## 5. Distribution

| Component | Size |
|---|---|
| Backend binary (Linux x86_64) | ~10 МБ |
| WASM core | ~300 КБ |
| TS bundle (gzipped) | < 200 КБ |
| **Total** | ~10.5 МБ |

Установка:
```bash
# Из исходников
cargo install looq

# Через single binary
curl -L https://github.com/.../releases/latest/download/looq-linux-x86_64 -o looq
chmod +x looq
```

## 6. CLI Interface

```bash
# Открыть файл
looq app.log

# Live tail из stdin
myapp | looq

# Или с явным флагом
tail -f /var/log/app.log | looq --stdin

# Настройка порта
looq --port 9000 app.log
looq --port 0 file.log    # случайный порт

# Авто-открытие браузера
looq --open file.log

# Версия
looq --version
```

**Все флаги:**
- `--port <u16>` — порт (default: 7891, 0 = random)
- `--host <ip>` — bind address (default: 127.0.0.1)
- `--open` — авто-открыть в браузере (default: false)
- `--no-browser` — заглушить auto-open (default: false)
- `--stdin` — force stdin mode (default: detect)
- `--max-lines <usize>` — лимит для live mode (default: 100_000)
- `--version`
- `--help`

## 7. Data Flow

### File mode
1. Клиент открывает `http://127.0.0.1:PORT`
2. Backend отдаёт `index.html` + `app.ts` + `core.wasm`
3. UI открывает файл через `<input type="file">` → File API
4. JS читает файл по частям (chunks), передаёт строки в WASM
5. WASM парсит, индексирует, возвращает структурированные записи
6. UI строит timeline, таблицу, применяет фильтры

### Stdin mode
1. CLI читает stdin построчно в отдельной tokio-task, пишет в bounded ring buffer на backend (размер = `--max-lines`) независимо от подключения клиентов — строки, пришедшие до открытия браузера, не теряются
2. Каждая строка отправляется по WebSocket всем подключённым клиентам; при новом подключении/reload backend сразу отдаёт snapshot текущего ring buffer, затем переходит в live-режим
3. Клиент передаёт в WASM для парсинга
4. Запись добавляется в таблицу без полной перерисовки
5. Если `max-lines` превышен, oldest строки удаляются (ring buffer, синхронно на backend и в клиентской таблице)
6. Backpressure: канал backend→клиент ограничен по размеру; медленный клиент не блокирует чтение stdin, но старые неотправленные сообщения могут быть отброшены с индикацией "gap" в UI

## 8. Format Detection

Алгоритм: первые 100 непустых строк проверяются regex-детекторами в порядке приоритета.

**MVP (M2, P0):**
1. **JSON Lines** — каждая строка валидный JSON
2. **logfmt** — паттерн `key=value` через пробел
3. **Plain text** — fallback, regex пользователя или эвристика

**P1 (после MVP, соответствует PRD F-3 core scope):**
4. **Syslog RFC 3164** — `<130>Aug  8 17:42:01 ...`
5. **Syslog RFC 5424** — `<130>1 2026-08-08T17:42:01Z ...`
6. **Apache/Nginx combined** — `IP - - [date] ...`
7. **Docker/k8s** — JSON wrapper вокруг контейнерного stdout

Приоритет проверки внутри каждой группы — в указанном порядке.

Пользователь может переопределить формат через URL hash: `#format=json` или `#format=logfmt`.

## 9. Field Extraction

Для каждого формата определён набор обязательных полей:
- `timestamp` — парсится через `chrono`, приоритет имён полей: `timestamp`, `ts`,
  `time`, `@timestamp`, `t`. Принимаются RFC 3339/ISO 8601 и epoch-значения
  (секунды/мс/мкс, по величине числа)
- `level` — из выделенного поля `level`/`lvl`/`severity`, иначе — скан текста
  сообщения по `TRACE|DEBUG|INFO|WARN|ERROR|FATAL` (регистронезависимо), с
  фиксированной таблицей алиасов (`WARNING`→`WARN`, `ERR`→`ERROR`,
  `CRITICAL`→`FATAL`); default-уровня нет — если ничего не найдено, level
  отсутствует
- `message` — основной текст; приоритет имён полей `message`, `msg`; если ни один
  не найден — logfmt использует "голые" токены (без `key=value`), JSON оставляет
  message пустым, а не дублирует туда всю строку

Произвольные поля извлекаются из JSON корня или из logfmt, становятся доступны как
фильтры, с ограничением по количеству различных значений на поле (cap подобран по
факту измерения памяти — `log-parsing-core`, `docs/devlog.md`); поле, упирающееся в
cap, помечается high-cardinality и перестаёт накапливать новые значения (сам счётчик
occurrences продолжает расти).

**Ограничение (не баг, а сознательно отложенный скоуп — `log-parsing-core`,
design.md D8):** вложенные JSON-объекты/массивы (`{"http":{"status":500}}`)
сохраняются как есть — полем `http` со значением сырого JSON-текста, а не
разворачиваются в `http.status`. Flattening — вероятное будущее развитие, но
требует решений по разделителю, индексам массивов, cap на глубину и коллизиям с
буквальными "точечными" ключами, которых в этом изменении не было.

**Ограничение: одна физическая строка = одна запись.** Многострочные payload'ы
(например, Java stack trace) дают N отдельных `Entry`, без агрегации — это
осознанное MVP-ограничение (PRD §14 Q3, P2), а не недоработка парсера.

Пользовательский custom regex — через URL hash: `#pattern=...`.

## 10. UI Surface

**Главный экран:**
- Top bar: путь к файлу, кнопки (Open File, Toggle Theme, Help)
- Timeline: гистограмма по count per bucket, drag для time-range
- Filter bar: full-text input + field filters (chips)
- Table: виртуальный скролл, columns: timestamp, level, message
- Detail panel (optional): выбранная строка с подсветкой полей

**Live tail indicator:**
- Зелёный dot в top bar при активном stdin
- Счётчик строк/сек

## 11. Performance Targets

- FCP (First Contentful Paint) < 500 мс на localhost
- Парсинг 1 МБ JSON в WASM < 200 мс
- Filter latency на 10k строк < 50 мс
- Live tail latency end-to-end < 100 мс
- Binary cold start < 100 мс (Linux x86_64)

## 12. Privacy Guarantees

- Backend не открывает файл (кроме случая `--enable-server-side-parse`, не в MVP)
- WebSocket передаёт **только stdin** в реальном времени
- Файл загружается через File API, остаётся в памяти браузера
- Никаких внешних запросов, никакой телеметрии, никакого CDN в runtime
- Можно отключить сеть после загрузки страницы — всё продолжит работать

**Важно — асимметрия file-mode vs stdin-mode:** в file-mode файл физически никогда не покидает память браузера. В stdin-mode (live tail) данные идут по localhost WebSocket в открытом виде между CLI-процессом и браузером — гарантия слабее ("не покидает машину", а не "не покидает браузер"). Это допустимый компромисс на localhost, но должно быть явно донесено до пользователя (UI-индикатор режима), а не смешиваться с формулировкой "файл не покидает браузер".

## 13. Security

- HTTP-сервер биндится только на `127.0.0.1` (не `0.0.0.0`)
- Флаг `--host` для изменения; при значении ≠ `127.0.0.1` — обязательный warning в stdout о том, что live-логи (WebSocket без auth) станут доступны всей сети
- WebSocket без auth (только localhost) — риск cross-site WebSocket hijacking: произвольная вредоносная страница в другой вкладке того же браузера может открыть `ws://127.0.0.1:PORT/ws` и читать stdin-поток. Митигация: проверка `Origin`-заголовка на бэкенде + одноразовый токен, встраиваемый в отдаваемый `index.html` и передаваемый при подключении к `/ws`
- CSP-заголовок от backend: `default-src 'self'` (покрывает `connect-src`/`worker-src`/`script-src` по умолчанию для same-origin WASM/comlink-воркера; перед релизом — кросс-браузерная проверка компиляции WASM под CSP, отдельные версии Safari/Firefox исторически требовали доп. директив)
- Потенциально позднее: флаг `--require-cors-origin` для запуска на удалённой машине

## 14. Risks

| Risk | Mitigation |
|---|---|
| WASM bundle большой | code splitting, lazy load heavy parsers |
| Парсинг гигантских файлов в браузере | **Решено (`release-hardening`).** Измерено (Chrome DevTools Protocol `Performance.getMetrics` вокруг `HeapProfiler.collectGarbage`, реальный файл-пикер, `bench-{50,100,200}mb.jsonl`): рост JS heap на главном потоке держится на удивление стабильно около 3.4x сырого размера файла на всех трёх точках (50 МБ → 3.40x, 100 МБ → 3.40x, 200 МБ → 3.39x) — это `entries: EntryDto[]` + `EntryIndex`, рабочее состояние на главном потоке, а не wasm32 linear memory: её собственное состояние (diagnostics, field inventory) ограничено `DEFAULT_DIAGNOSTIC_CAP`/`DEFAULT_FIELD_VALUE_CAP` (`log-parsing-core`) и не растёт с размером файла. Время парсинга росло линейно, ~80 мс/МБ на этих размерах — с большим запасом внутри цели §11 (<200 мс/МБ) даже на 200 МБ (замерено: 15 970.6 мс). Warning threshold — 50 МБ (~3с, ~170 МБ heap); hard cap — 400 МБ (при коэффициенте 3.4x это ~1.36 ГБ JS heap и ~32с парсинга — заметное ожидание, но заметно ниже точки, где в этом окружении проявились бы проблемы: 200 МБ прошли чисто при 678 МБ heap без признаков деградации). Выше hard cap — явный отказ с объяснением, а не попытка распарсить. Точные числа и команда — `docs/devlog.md`, запись `release-hardening`. |
| `wasm-bindgen` overhead | минимизировать use `serde-wasm-bindgen` |
| Совместимость tsify ↔ WASM | строгий CI, type-check через `tsc --noEmit` |
| Firewall блокирует localhost | `--host 0.0.0.0` с явным флагом + обязательный warning (см. §13 — без auth на WebSocket это открывает live-логи всей сети) |
| Потеря/отброс stdin-строк при отсутствии/медленности клиента | bounded ring buffer на backend + snapshot при (пере)подключении (см. §7) |

## 15. Milestones

> Эстимейты предполагают full-time работу выделенной команды/разработчика. WASM-core + JS-interop и UI (timeline/virtual scroll/фильтры) на практике часто занимают 2+ недели каждый по отдельности — 6 недель суммарно агрессивный, а не консервативный план. Считать риском срыва, не гарантией.

**M1 — Backend skeleton (1 неделя):**
- `cargo new`, axum hello world
- Static embed, открытие в браузере
- WebSocket echo для stdin

**M2 — WASM core (2 недели):**
- Парсеры JSON, logfmt, plain
- Структуры Entry, Index
- JS interop, tests

**M3 — UI MVP (2 недели):**
- TS scaffold, Web Components
- Timeline (uPlot), virtual table
- Filters, search

**M4 — Polish (1 неделя):**
- Themes, error states
- Docs, README, demo video
- Release 0.1.0

## 16. Open Questions

- Нужен ли export (filtered JSON/CSV) в MVP?
- ~~WebSocket бинарный vs текстовый формат?~~ **Решено (`live-tail`, design.md
  D1):** текстовый JSON envelope с тегом `type`: `line`, `snapshot`, `gap`,
  `ended`. Сырой неразмеченный текст строки больше не отправляется — даже если
  содержимое строки лога само похоже на envelope, оно передаётся как значение
  поля `text`, а не интерпретируется. Бинарный формат был бы компактнее и
  быстрее парсился, но признан преждевременной оптимизацией: объём, при
  котором это стало бы заметно, — это тот же объём, при котором ring buffer уже
  отбрасывает строки backpressure'ом; а текстовый протокол проверяется
  `wscat`/любым WS-клиентом, на чём построены все тесты этого изменения.
  Зафиксирован измеримый триггер пересмотра: если сериализация envelope
  проявится в профиле на целевой пропускной способности — переключиться на
  бинарный формат. Замер (`crates/looq/tests/cli.rs`,
  `snapshot_at_default_max_lines_is_delivered_promptly`, release-сборка):
  snapshot на 100 000 строк (~110 байт/строка, ~12.7 МБ JSON) доставляется за
  ~81 мс одним WS-сообщением — чанкинг (D3, запасной план) не потребовался.
- ~~Поддержка таймзон в timestamp — fallback на UTC?~~ **Решено
  (`log-parsing-core`, design.md D5):** timestamp с явным offset интерпретируется
  как есть; timestamp без offset — в таймзоне, заданной вызывающей стороной, по
  умолчанию UTC; какая интерпретация была применена — возвращается вместе с
  результатом, чтобы UI мог показать пользователю, какое допущение сделано.
  Именованные IANA-зоны (`Europe/Belgrade`) не поддержаны — только
  фиксированный UTC-offset; полная зона потребовала бы `chrono-tz` и её
  embedded-данных, которые сами по себе (по аналогии с `regex`, см.
  `docs/devlog.md`) почти наверняка не влезли бы в бюджет `core.wasm` (§5). Это
  сознательное сужение скоупа, отмеченное как NEEDS HUMAN DECISION в отчёте по
  `log-parsing-core` — решение о полноценных именованных зонах ещё предстоит
  подтвердить человеку.
- ~~Поведение при невалидных строках — skip / warn / fail?~~ **Решено
  (`log-parsing-core`, design.md D4, PRD §14 Q2):** skip + warn. Строка
  пропускается, парсинг продолжается, диагностика (номер строки, причина)
  фиксируется. Диагностики ограничены и агрегированы: индивидуальные примеры
  хранятся до фиксированного cap'а (5 000 — подобран по измерению памяти,
  `docs/devlog.md`), точные счётчики по каждой причине продолжают расти после
  cap'а без просадки памяти — файл с миллионом невалидных строк даёт "1 000 000
  строк пропущено: invalid JSON" плюс первые N примеров, а не миллион
  индивидуальных предупреждений и не пустой результат.

## 17. MCP Server Mode (P2)

### Что это

Режим `--mcp` запускает **stdio MCP server** (Model Context Protocol), через который AI-агенты (Claude Desktop, Cursor, Cline, и т.д.) могут вызывать инструменты `looq_*` для чтения лог-файла пользователя.

```
┌──────────────────┐  stdio JSON-RPC  ┌──────────────────┐
│ AI Agent         │ ───────────────► │ looq --mcp       │
│ (Claude Desktop) │                  │  ├── core lib    │
│                  │ ◄─────────────── │  └── file parser │
└──────────────────┘   structured     └──────────────────┘
                       responses              │
                                               │ file read
                                               ▼
                                    /var/log/app.log
                                    (остаётся на машине)
```

**Важно — другая privacy-модель, не "тот же принцип, но через IPC":** в web-режиме (§12) backend файл не открывает вообще — парсинг идёт в браузерном WASM-сандбоксе. В MCP-режиме бэкенд-процесс **сам читает файл нативно** (`std::fs`), в обход браузера и WASM-изоляции. Это сознательный компромисс: данные остаются на машине пользователя (нет внешней сети), но модель доступа принципиально иная — файл открывает нативный процесс, а не браузерная песочница. Формулировка "privacy-инвариант сохраняется" в MVP-документации должна явно означать "не покидает машину", а не "не покидает браузер".

### Tools

| Tool | Args | Returns |
|---|---|---|
| `looq_open` | `path: string` | `entry_count, format_detected, time_range` |
| `looq_query` | `filter?: string, range?: [ts_from, ts_to], limit?: number` | `entries: [{ts, level, message, fields}]` |
| `looq_summarize` | `range?: [ts_from, ts_to]` | `stats: {by_level, top_errors, anomalies}` |
| `looq_list_files` | `glob: string` | `paths: [string]` |

### Transport

- **stdio JSON-RPC 2.0** — стандарт MCP
- Библиотека: `rmcp` (официальный Rust SDK, https://github.com/modelcontextprotocol/rust-sdk, крейт `rmcp` + `rmcp-macros` на crates.io) или минимальный свой клиент (~200 строк, JSON-RPC + Content-Length framing)
- Никакого HTTP/WebSocket в этом режиме — порт не занимается, конфликтов нет

### CLI

```bash
# Запуск MCP server для конкретного файла
looq --mcp /var/log/app.log

# MCP server + web UI одновременно (два процесса, share через WASM)
looq --mcp --web app.log
```

### Use case

```
User (в Claude Desktop): "Что упало в app.log за последний час?"
Claude: → looq_open("/var/log/app.log")
       → looq_query(range=["2026-08-08T20:00","2026-08-08T21:00"])
       → looq_summarize(...)
       → отвечает пользователю структурированным объяснением
```

User не копипастит лог в чат — агент читает его локально через MCP.

### Tech additions

- `rmcp` SDK — размер добавки к бинарнику не оценён, требует замера (уже есть tokio/serde_json в зависимостях, инкрементальный вес ниже номинального размера крейта, но декларировать цифру до бенчмарка не стоит)
- Или своя реализация JSON-RPC framing на `tokio::io::stdin()`
- `serde_json` уже есть в зависимостях
- **Реюз парсинга:** переиспользуется не `.wasm`-артефакт (он собран под `wasm32-unknown-unknown` с `wasm-bindgen`/`web-sys` и не исполняется вне браузера без WASM-рантайма типа wasmtime, которого нет в стеке), а сама Rust-логика парсинга — для этого core-парсер должен жить в отдельном target-agnostic крейте (без `wasm-bindgen`/`web-sys` зависимостей), с двумя тонкими адаптерами: `wasm-bindgen`-обвязка для браузера и нативный адаптер для MCP/CLI

### Risks

| Risk | Mitigation |
|---|---|
| MCP SDK нестабилен | минимальная своя реализация как fallback |
| Агент шлёт гигантские ответы | `limit` параметр (default 100), `range` обязателен для `query` |
| Path traversal через `looq_open` | reject absolute paths outside cwd + `$LOOQ_ALLOWED_DIRS` env var |
| Агент пытается читать `/etc/shadow` через `looq_list_files` | glob валидация + audit log всех файловых доступов |
