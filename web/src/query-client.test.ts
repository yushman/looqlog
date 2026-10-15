import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ConnectionLostError, QueryClient, RequestError, type ConnectionState, type SocketLike } from "./query-client";
import type { Notification } from "./query-types";
import { ViewSession } from "./view-session";

class FakeSocket implements SocketLike {
  sent: string[] = [];
  private listeners = new Map<string, Array<(event: { data: unknown }) => void>>();
  closed = false;

  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    this.closed = true;
  }
  addEventListener(type: string, listener: (event: { data: unknown }) => void): void {
    const list = this.listeners.get(type) ?? [];
    list.push(listener);
    this.listeners.set(type, list);
  }
  emit(type: string, data?: unknown): void {
    for (const l of this.listeners.get(type) ?? []) {
      l({ data });
    }
  }
  server(msg: object): void {
    this.emit("message", JSON.stringify(msg));
  }
  lastRequest(): { id: number; type: string } & Record<string, unknown> {
    return JSON.parse(this.sent[this.sent.length - 1]!);
  }
}

function setup() {
  const sockets: FakeSocket[] = [];
  const states: ConnectionState[] = [];
  const notes: Notification[] = [];
  let connected = 0;
  const client = new QueryClient({
    url: "ws://x/ws",
    token: "tok",
    onState: (s) => states.push(s),
    onNotification: (n) => notes.push(n),
    onConnected: () => {
      connected += 1;
    },
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
  });
  client.connect();
  const live = (s: FakeSocket) => {
    s.emit("open");
    s.server({ type: "grown", total: 0, matched: 0, rowCount: 0 });
  };
  return { client, sockets, states, notes, live, connectedCount: () => connected };
}

describe("QueryClient", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("sends the token as the first frame and goes live on the first message", () => {
    const { sockets, states, live, connectedCount } = setup();
    live(sockets[0]!);
    expect(JSON.parse(sockets[0]!.sent[0]!)).toEqual({ type: "auth", token: "tok" });
    expect(states).toEqual(["live"]);
    expect(connectedCount()).toBe(1);
  });

  it("matches replies to requests by id", async () => {
    const { client, sockets, live } = setup();
    live(sockets[0]!);
    const a = client.request({ type: "window", offset: 0, limit: 1 });
    const idA = sockets[0]!.lastRequest().id;
    const b = client.request({ type: "window", offset: 1, limit: 1 });
    const idB = sockets[0]!.lastRequest().id;
    sockets[0]!.server({ type: "reply", id: idB, data: "b" });
    sockets[0]!.server({ type: "reply", id: idA, data: "a" });
    await expect(a).resolves.toBe("a");
    await expect(b).resolves.toBe("b");
  });

  it("rejects an error reply with its message", async () => {
    const { client, sockets, live } = setup();
    live(sockets[0]!);
    const p = client.request({ type: "stats" });
    sockets[0]!.server({ type: "error", id: sockets[0]!.lastRequest().id, message: "nope" });
    await expect(p).rejects.toThrow(RequestError);
  });

  it("fails pending requests on close and reconnects with backoff", async () => {
    const { client, sockets, states, live, connectedCount } = setup();
    live(sockets[0]!);
    const p = client.request({ type: "stats" });
    sockets[0]!.emit("close");
    await expect(p).rejects.toThrow(ConnectionLostError);
    expect(states[states.length - 1]).toBe("disconnected");
    vi.advanceTimersByTime(500);
    expect(sockets.length).toBe(2);
    live(sockets[1]!);
    expect(connectedCount()).toBe(2);
    expect(states[states.length - 1]).toBe("live");
  });

  it("passes notifications through and reports a stream end", () => {
    const { sockets, notes, states, live } = setup();
    live(sockets[0]!);
    sockets[0]!.server({ type: "evicted", count: 5 });
    sockets[0]!.server({ type: "ended" });
    expect(notes.map((n) => n.type)).toEqual(["grown", "evicted", "ended"]);
    expect(states[states.length - 1]).toBe("ended");
  });
});

describe("ViewSession", () => {
  function fakeRequester(replies: Record<string, (req: Record<string, unknown>) => unknown>) {
    return {
      request<T>(req: { type: string }): Promise<T> {
        return Promise.resolve(replies[req.type]!(req as Record<string, unknown>) as T);
      },
    };
  }

  const queryReply = (generation: number, extra: object = {}) => ({
    generation,
    total: 10,
    matched: 4,
    rowCount: 3,
    timestamplessExcluded: 0,
    tokens: [],
    remainingText: "",
    compileError: null,
    ...extra,
  });

  it("discards a window computed for an older generation", async () => {
    let windowGeneration = 1;
    const session = new ViewSession(
      fakeRequester({
        query: () => queryReply(2),
        window: () => ({ generation: windowGeneration, offset: 0, rowCount: 3, matched: 4, total: 10, rows: [] }),
      }),
    );
    await session.query({ filters: new Map(), search: "x", range: null });
    expect(await session.window(0, 10)).toBeNull();
    windowGeneration = 2;
    expect(await session.window(0, 10)).not.toBeNull();
  });

  it("keeps the previous generation and search on a compile error", async () => {
    let reply = queryReply(1, { remainingText: "ok" });
    const session = new ViewSession(fakeRequester({ query: () => reply }));
    await session.query({ filters: new Map(), search: "ok", range: null });
    reply = queryReply(1, { compileError: "invalid regex: unclosed" });
    const r = await session.query({ filters: new Map(), search: "re:[", range: null });
    expect(r.compileError).toContain("invalid regex");
    expect(session.currentGeneration).toBe(1);
    expect(session.state.search).toBe("ok");
  });

  it("folds extracted field tokens into the applied filters", async () => {
    const session = new ViewSession(
      fakeRequester({
        query: () => queryReply(1, { tokens: [{ field: "service", value: "api" }], remainingText: "timeout" }),
      }),
    );
    await session.query({ filters: new Map(), search: "service=api timeout", range: null });
    expect([...(session.state.filters.get("service") ?? [])]).toEqual(["api"]);
    expect(session.state.search).toBe("timeout");
    expect(session.hasFilters).toBe(true);
  });

  it("sends and tracks chain toggles", async () => {
    const sent: Array<Record<string, unknown>> = [];
    const session = new ViewSession(
      fakeRequester({
        toggle_chain: () => ({ rowCount: 7 }),
        query: (req) => {
          sent.push(req);
          return queryReply(1);
        },
      }),
    );
    expect(await session.toggleChain(4)).toBe(7);
    await session.query({ filters: new Map(), search: "", range: null });
    expect(sent[0]!.toggled).toEqual([4]);
  });
});
