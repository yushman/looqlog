# PRD — looqlog

**Version:** 1.0
**Date:** 2026-08-08
**Status:** Draft
**Owner:** looqlog maintainers

---

## 1. Vision

Сделать просмотр логов таким же простым, как открыть PDF. Один binary, ноль конфигурации, ноль серверов, ноль утечек данных.

## 2. Problem

При разборе инцидентов разработчик регулярно делает:
```bash
kubectl logs pod-1 > incident.log
docker logs app > incident.log
myapp > app.log
```

И дальше вынужден выбирать из неудовлетворительных опций:
- **less / grep / awk** — нет timeline, нет фильтров, нет структуры
- **lnav** — мощный, но TUI, не все фичи очевидны, нет privacy-режима и web UI для расшаривания диапазона по URL
- **VS Code** — медленно на больших файлах, не UI для логов
- **Grafana / Kibana** — требует инфраструктуры, нет privacy-режима
- **Онлайн-сервисы** — отправляет чувствительные данные на чужой сервер

Нужен инструмент, который:
- Запускается одной командой
- Открывает файл или stdin
- Показывает timeline, фильтры, поиск
- Работает локально, без сетевых запросов
- Сам открывает браузер

## 3. Users

### Primary: Backend-разработчик / SRE
- **Контекст:** разбирает инцидент, есть 1–10 лог-файлов, нужно за 5 минут найти причину
- **Боли:** `grep` не структурный, Kibana недоступна, lnav неудобен в плане UX
- **Готовность:** установит single binary, откроет в браузере, готов перейти на CLI

### Secondary: DevOps / Platform engineer
- **Контекст:** ежедневно смотрит логи от 10+ сервисов, drill-down по trace_id
- **Боли:** хочет telemetry-осведомлённый viewer без запуска open-source observability stack
- **Готовность:** готов контрибьютить, если архитектура ясная

### Tertiary: Security / Privacy-параноик
- **Контекст:** лог-файлы содержат credentials, PII, нельзя отправлять на внешний сервис
- **Боли:** большинство viewer'ов = SaaS или требует инфраструктуры
- **Готовность:** оценит WASM-only режим

## 4. Product Principles

1. **Privacy first.** Файл не покидает машину. WASM парсит локально.
2. **Zero config.** Один binary, одна команда, открылся в браузере.
3. **Single binary.** Никаких npm install, никаких docker-compose, никаких зависимостей.
4. **Honest tech.** Rust для надёжности, TypeScript для UI, без магии.
5. **Free forever.** MIT, без telemetry, без paid tiers.

> Примечание: "single binary" / "zero dependencies" относится к **runtime** — конечный пользователь не ставит ничего, кроме бинаря. Для сборки из исходников нужны Rust toolchain, Node.js и wasm-pack (см. §11 Dependencies).

## 5. User Stories

### US-1: Открыть файл локально
**Как** разработчик, **я хочу** открыть `app.log` в нормальном UI, **чтобы** не использовать `grep`/`awk`/`less`.

**Acceptance:**
- `looqlog app.log` запускает сервер и открывает браузер на `http://127.0.0.1:7891`; страница
  показывает `app.log` как подсказку, какой файл выбрать — сам процесс `looqlog` файл не
  открывает и не читает (см. ADR-0007: ни один browser API не может открыть путь,
  переданный сервером, без жеста пользователя)
- Пользователь выбирает файл через file picker или drag-and-drop → файл парсится,
  появляется timeline + таблица
- Auto-detect формата (JSON, logfmt, plain)
- Файл НЕ отправляется на сервер (загружен через File API)

### US-2: Live tail из stdin
**Как** разработчик, **я хочу** видеть логи приложения в реальном времени, **чтобы** не перезапускать tail каждый раз.

**Acceptance:**
- `myapp | looqlog` запускает сервер
- Строки из stdin появляются в таблице без перезагрузки
- Latency < 100 мс на localhost
- Ring buffer: при превышении лимита старые строки удаляются

### US-3: Фильтр по времени
**Как** разработчик, **я хочу** выделить диапазон на timeline, **чтобы** сузить область поиска.

**Acceptance:**
- Timeline показывает count per bucket
- Drag по timeline → фильтр по диапазону
- Range отображается в URL hash для шаринга

### US-4: Фильтр по полям
**Как** разработчик, **я хочу** отфильтровать `level=ERROR service=api`, **чтобы** убрать шум.

**Acceptance:**
- Доступные поля показаны как chips
- Поиск поддерживает синтаксис `field=value`
- Подсветка совпадений в таблице

### US-5: Полнотекстовый поиск
**Как** разработчик, **я хочу** искать по подстроке, **чтобы** найти конкретную ошибку.

**Acceptance:**
- Search input с подсветкой совпадений
- Регистронезависимый по умолчанию
- Поддержка regex через префикс `re:`

### US-6: Privacy-режим
**Как** security-инженер, **я хочу** убедиться, что файл не уходит на сервер, **чтобы** открывать логи с credentials.

**Acceptance:**
- Network tab в DevTools пустой после загрузки страницы
- Сервер биндится на 127.0.0.1
- CSP запрещает external resources

### US-7: Один binary для команды
**Как** тимлид, **я хочу** чтобы установка была тривиальной, **чтобы** команда не тратила время на setup.

**Acceptance:**
- `cargo install looqlog` или готовый бинарь
- README с примерами на 1 страницу
- Demo video < 2 минут

### US-8: Доступ из AI-агента (P2)
**Как** разработчик с Cursor/Claude Desktop, **я хочу** чтобы агент мог открыть лог-файл через MCP и ответить на вопрос "что упало в последний час?", **чтобы** не копипастить логи в чат руками.

**Acceptance:**
- `looqlog --mcp` запускает MCP server на stdio
- Агент может вызвать `looqlog_open(path)`, `looqlog_query(filter, range)`, `looqlog_summarize(range)`
- Файл не покидает машину (MCP = локальный IPC, не HTTP вовне)
- Privacy гарантии из US-6 сохраняются

## 6. Features

### MVP (v0.1.0)

| ID | Feature | Priority |
|---|---|---|
| F-1 | CLI: open file by path | P0 |
| F-2 | CLI: stdin pipe | P0 |
| F-3 | Auto-detect JSON / logfmt / plain | P0 |
| F-4 | Timeline with time-range drag | P0 |
| F-5 | Virtual-scrolled table | P0 |
| F-6 | Field-based filters (level, service) | P0 |
| F-7 | Full-text search | P0 |
| F-8 | Light/Dark theme | P1 |
| F-9 | Live tail with WebSocket | P0 |
| F-10 | WASM parsing core | P0 |
| F-11 | TypeScript UI layer | P0 |
| F-12 | Single binary distribution | P0 |
| F-13 | File API upload (no server-side read) | P0 |
| F-14 | CLI auto-open browser | P1 |
| F-15 | URL hash state (#range=, #filter=) | P1 |
| F-16 | MCP server mode (agent integration) | P2 |

### P2 (v0.2.0)
- gzip/zstd decompression
- Trace correlation (trace_id, span_id)
- Multi-file merge
- Export filtered results (JSON / CSV)
- Saved views / persistent filters
- **MCP server** для интеграции с Claude/Cursor/другими агентами

### P3 (v0.3.0+)
- SQL queries (lnav-style)
- Custom regex patterns via config
- SSH remote tail
- Plugin system for formats
- Cathode-ray / terminal-style theme

## 7. UX Flows

### Flow 1: First run

```
$ looqlog app.log
┌────────────────────────────────────────┐
│  looqlog v0.1.0                           │
│  → http://127.0.0.1:7891               │
│  Press Ctrl+C to quit                  │
└────────────────────────────────────────┘
```

Браузер открывается автоматически (если `--open`). Файл выбран через File API.

### Flow 2: Live tail

```
$ myapp | looqlog --open
[streaming...]
```

Top bar показывает зелёный indicator `LIVE`, счётчик строк/сек. Таблица autoscroll'ится с throttling.

### Flow 3: Filter

```
1. Click chip [ERROR] → таблица фильтруется
2. Click chip [service=api] → сужается
3. Drag timeline → диапазон установлен
4. URL hash: #range=2026-08-08T17:30..17:45&filter=level=ERROR,service=api
5. Share URL коллеге (на той же машине)
```

### Flow 4: Search

```
1. Type "connection refused" в search bar
2. Совпадения подсвечиваются, таблица фильтруется
3. Press `re:^ERROR.*timeout` → regex search
4. Esc → clear
```

## 8. Out of Scope (v0.1.0)

- Backend-database indexing (Loki, ClickHouse)
- Distributed/multi-tenant
- Alerting / dashboards
- Saved queries / queries library
- Real-time collaborative features
- Mobile layout
- Cloud sync / sharing
- Authentication / multi-user

## 9. Success Metrics

| Metric | Target (v0.1) | Target (v1.0) |
|---|---|---|
| GitHub stars | 500 | 5000 |
| Monthly downloads | 1000 | 10000 |
| TTI (time to interactive) | < 1 s | < 500 ms |
| Memory at 10k lines | < 100 MB | < 50 MB |
| FCP at localhost | < 500 ms | < 300 ms |

Stars/downloads — directional таргеты, не гарантированный план: при zero-telemetry (принцип §4) их источник — публичная статистика GitHub/crates.io, дистрибуция зависит от launch-плана (HN/Reddit, см. §13 Future Roadmap v1.0), которого в MVP-скоупе нет.

## 10. Architecture Recap

(См. TDR.md § 3 — диаграмма и поток данных.)

## 11. Dependencies

### External
- Web browsers: Chrome 110+, Firefox 110+, Safari 16+
- WASM: поддержка в браузерах с 2017
- TCP-порт 7891 (или случайный)

### Internal
- Rust toolchain 1.86+
- Node.js 20+ (dev only, для сборки WASM/TS)
- `wasm-pack` (dev)

### Runtime
- Zero. Single binary запускается на любой Linux/macOS/Windows.

## 12. Risks & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| WASM парсинг медленнее нативного | High | columnar layout, SIMD, benchmarks |
| Browser не открывается (WSL, headless, SSH на remote-сервер без GUI — типичный сценарий инцидента для SRE из §3) | Medium | `--no-browser` флаг, manual URL + вывод команды `ssh -L PORT:127.0.0.1:PORT` для port-forward в help/stdout при отсутствии GUI |
| Файл > 100 МБ в браузере | High | warning UI, chunked loading |
| Privacy-течь по WebSocket | Critical | WebSocket только из stdin, не из файла. Важно: stdin-данные (live tail) физически идут по localhost WebSocket в открытом виде — это более слабая гарантия, чем file-mode (файл никогда не покидает память браузера). Документировать эту асимметрию для пользователя |
| `--host 0.0.0.0` + WebSocket без auth | Critical | При смене bind-адреса live-логи (в т.ч. с credentials/PII) становятся доступны всей локальной сети без аутентификации. Обязательный warning в CLI при `--host` ≠ 127.0.0.1 (см. TDR §13) |
| input file encoding | Medium | utf-8 by default, latin-1 fallback |
| Timezone parsing | Medium | UTC по умолчанию, override в URL hash |

## 13. Future Roadmap

- **v0.2.0** — gzip/zstd, trace correlation, export
- **v0.3.0** — SQL queries, multi-file merge
- **v0.4.0** — Plugin system, custom themes
- **v0.5.0** — SSH remote tail, kube-rs integration
- **v1.0** — stable API, 1.0 release post on HN/Reddit

## 14. Open Questions

1. Нужен ли **export** в MVP? (P1 vs P2)
2. ~~Поведение при **невалидных строках**: skip / warn / fail?~~ **Решено
   (`log-parsing-core`):** skip + warning. Строка пропускается без записи,
   парсинг продолжается, диагностика с номером строки и причиной сохраняется
   (ограниченно и агрегированно — см. TDR §16, `docs/devlog.md`). Молчаливое
   отбрасывание строки без диагностики — дефект, а не приемлемое поведение.
3. ~~Поддержка **stack traces** — multi-line aggregation? (P2)~~ **Решено
   (`multiline-entry-continuations`):** mark, don't merge. Каждая физическая строка
   по-прежнему даёт свою запись, но несёт ссылку на корень цепочки; распознавание —
   только назад (никакого lookahead, live-режим не задерживает ни одной строки).
   Признаки строго положительные: явный маркер фрейма, повтор logcat-префикса с тем же
   `(pid, tid, level, tag)` или незакрытая `{`; цепочка открывается только под строкой
   с распознанным таймстампом. В таблице — сворачиваемая группа, на timeline — одно
   событие, а не строка. Цепочка длиннее 1000 строк закрывается с диагностикой
   `chain_truncated`. Ключи внутри многострочного JSON-payload'а полями не становятся —
   это следствие модели, а не недоделка (design D7).
4. **Auto-open browser** — default on или off? (по умолчанию off, чтобы не пугать)
5. **Bind address** — 127.0.0.1 only или опция 0.0.0.0? (127.0.0.1 + opt-in flag)
6. **MCP как P2 или ускорить в P1?** Аргументы за P1: тренд на agentic workflows растёт, дифф с конкурентами. Аргументы за P2: MVP должен остаться минимальным, MCP требует доп. тестирования интеграций. **Решение зафиксировано: P2.** TDR §17 описывает дизайн заранее (black-box, без обязательств на MVP-release), чтобы не блокировать архитектуру ядра парсинга при будущей реализации.
7. **Локальный AI summarizer** (WebLLM/transformers.js в WASM) — пилить как часть F-16 или отдельная фича?
