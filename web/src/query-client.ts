// The one `/ws` connection a tab holds (`query-api` spec, ADR-0010): authenticates with
// the per-process token as its first frame, sends requests tagged with ids, resolves
// each with its reply, hands notifications to the shell, and reconnects with capped
// exponential backoff. No DOM, so it is unit-testable with a fake socket.

import type { ClientRequest, Notification, ServerMessage } from "./query-types";

export type ConnectionState = "connecting" | "live" | "ended" | "disconnected";

/** The part of `WebSocket` this client uses, so tests can substitute a fake. */
export interface SocketLike {
  send(data: string): void;
  close(): void;
  addEventListener(type: "open" | "close" | "error", listener: () => void): void;
  addEventListener(type: "message", listener: (event: { data: unknown }) => void): void;
}

export class RequestError extends Error {}

/** A request whose connection went away before the reply arrived. */
export class ConnectionLostError extends Error {
  constructor() {
    super("the connection to looqlog was lost");
  }
}

export interface QueryClientOptions {
  url: string;
  token: string;
  onState: (state: ConnectionState) => void;
  onNotification: (notification: Notification) => void;
  /** Called after every (re)authentication — the shell re-issues its current query
   * and window here, so a reconnect shows each retained entry exactly once. */
  onConnected: () => void;
  createSocket?: (url: string) => SocketLike;
}

const RECONNECT_BASE_MS = 500;
const RECONNECT_MAX_MS = 10_000;

interface Pending {
  resolve: (data: unknown) => void;
  reject: (err: Error) => void;
}

export class QueryClient {
  private socket: SocketLike | null = null;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private state: ConnectionState = "connecting";
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;
  private ended = false;

  constructor(private readonly opts: QueryClientOptions) {}

  connect(): void {
    this.disposed = false;
    this.open();
  }

  getState(): ConnectionState {
    return this.state;
  }

  /** Sends `req` and resolves with the reply's data; rejects with `RequestError` on
   * an error reply and `ConnectionLostError` if the socket closes first. */
  request<T>(req: ClientRequest): Promise<T> {
    const socket = this.socket;
    if (socket === null || this.state === "connecting" || this.state === "disconnected") {
      return Promise.reject(new ConnectionLostError());
    }
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: (data) => resolve(data as T), reject });
      socket.send(JSON.stringify({ ...req, id }));
    });
  }

  dispose(): void {
    this.disposed = true;
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.socket?.close();
    this.socket = null;
    this.failPending();
  }

  private open(): void {
    this.setState("connecting");
    let socket: SocketLike;
    try {
      socket = (this.opts.createSocket ?? ((url) => new WebSocket(url)))(this.opts.url);
    } catch {
      this.setState("disconnected");
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({ type: "auth", token: this.opts.token }));
    });
    socket.addEventListener("message", (event) => {
      if (this.socket !== socket) {
        return;
      }
      if (this.state === "connecting") {
        // The server's first frame after a valid token is a notification with totals.
        this.reconnectAttempt = 0;
        this.setState(this.ended ? "ended" : "live");
        this.opts.onConnected();
      }
      this.handle(String(event.data));
    });
    socket.addEventListener("close", () => {
      if (this.socket !== socket) {
        return;
      }
      this.socket = null;
      this.failPending();
      if (this.disposed) {
        return;
      }
      this.setState("disconnected");
      this.scheduleReconnect();
    });
    // 'error' is always followed by 'close'; reconnecting lives there.
    socket.addEventListener("error", () => undefined);
  }

  private handle(raw: string): void {
    let msg: ServerMessage;
    try {
      msg = JSON.parse(raw) as ServerMessage;
    } catch {
      console.error("looqlog: could not parse server message:", raw);
      return;
    }
    switch (msg.type) {
      case "reply": {
        const p = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        p?.resolve(msg.data);
        return;
      }
      case "error": {
        const p = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        p?.reject(new RequestError(msg.message));
        return;
      }
      case "ended":
        this.ended = true;
        this.setState("ended");
        break;
    }
    this.opts.onNotification(msg);
  }

  private failPending(): void {
    const pending = [...this.pending.values()];
    this.pending.clear();
    for (const p of pending) {
      p.reject(new ConnectionLostError());
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) {
      return;
    }
    const delay = Math.min(RECONNECT_BASE_MS * 2 ** this.reconnectAttempt, RECONNECT_MAX_MS);
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.disposed) {
        this.open();
      }
    }, delay);
  }

  private setState(state: ConnectionState): void {
    if (this.state === state) {
      return;
    }
    this.state = state;
    this.opts.onState(state);
  }
}

/** `ws(s)://<this page's host>/ws`. */
export function socketUrl(): string {
  return `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ws`;
}
