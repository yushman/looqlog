# looqlog

Single-binary CLI that opens a local web UI for browsing a log file or a live stdin
stream. Parsing happens in WebAssembly inside your own browser.

```sh
cargo install looqlog
looqlog /var/log/app.log      # file mode
kubectl logs -f pod | looqlog # live stdin
```

Auto-detects JSON Lines, logfmt and plain text, with syslog, klog, Apache/CLF, Docker
and Android logcat prefixes read through the plain-text path. Gives you a histogram
timeline with drag-to-select, a virtual-scrolled table, filter chips, full-text and
regex search, and a shareable URL that encodes the view.

**Privacy, stated precisely.** In file mode the log is read by the browser's File API
and never reaches the backend at all — it does not leave the browser. In stdin mode
lines travel from the CLI process to the browser over a localhost WebSocket, guarded by
an origin check and a per-process token: it does not leave your machine, but it does
cross a process boundary. The two modes do not carry the same guarantee.

Full documentation — flags, supported formats, security model, file-size limits and
known limitations — lives in the repository:

- [README](https://github.com/yushman/looqlog#readme) ·
  [Русская версия](https://github.com/yushman/looqlog/blob/main/README.ru.md)
- [Architecture decisions](https://github.com/yushman/looqlog/tree/main/docs/adr)

MIT licensed.
