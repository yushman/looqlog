# looqlog

Single-binary CLI that opens a local web UI for browsing a log file or a live stdin
stream. Parsing happens in the local `looqlog` process; the browser only shows it.

```sh
cargo install looqlog
looqlog /var/log/app.log      # file mode
kubectl logs -f pod | looqlog # live stdin
```

Auto-detects JSON Lines, logfmt and plain text, with syslog, klog, Apache/CLF, Docker
and Android logcat prefixes read through the plain-text path. Gives you a histogram
timeline with drag-to-select, a virtual-scrolled table, filter chips, full-text and
regex search, and a shareable URL that encodes the view.

**Privacy, stated precisely.** In both modes the log — an argv file, a file uploaded from
the page, or stdin — is read by the local `looqlog` process and reaches the browser over a
localhost connection guarded by origin/host checks and a per-process token. It does not
leave your machine on the default bind, but it does cross a process boundary; nothing
claims it stays inside the browser.

Full documentation — flags, supported formats, security model, file-size limits and
known limitations — lives in the repository:

- [README](https://github.com/yushman/looqlog#readme) ·
  [Русская версия](https://github.com/yushman/looqlog/blob/main/README.ru.md)
- [Architecture decisions](https://github.com/yushman/looqlog/tree/main/docs/adr)

MIT licensed.
