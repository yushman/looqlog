// This tab's view over the backend (`query-api` spec): the query it last applied, the
// generation that query started, the chain toggles, and the latest counts. Replies
// computed for an older generation are discarded here, so no component ever renders a
// window that belongs to a superseded filter.

import type {
  BucketsReplyDto,
  ClientRequest,
  EntryReplyDto,
  QueryReplyDto,
  StatsDto,
  StatsReplyDto,
  WindowReplyDto,
} from "./query-types";
import type { TimeRange } from "./time-range";

export type FieldFilters = ReadonlyMap<string, ReadonlySet<string>>;

export interface ViewState {
  filters: FieldFilters;
  search: string;
  range: TimeRange | null;
}

export interface Counts {
  total: number;
  matched: number;
  rowCount: number;
  timestamplessExcluded: number;
}

export interface Requester {
  request<T>(req: ClientRequest): Promise<T>;
}

export class ViewSession {
  private generation = 0;
  private toggled = new Set<number>();
  private applied: ViewState = { filters: new Map(), search: "", range: null };
  counts: Counts = { total: 0, matched: 0, rowCount: 0, timestamplessExcluded: 0 };

  constructor(private readonly client: Requester) {}

  get currentGeneration(): number {
    return this.generation;
  }

  get state(): ViewState {
    return this.applied;
  }

  get hasFilters(): boolean {
    return [...this.applied.filters.values()].some((v) => v.size > 0) || this.applied.search.length > 0;
  }

  /** Applies `state`. On a compile error the previous generation stays current and
   * the reply says why; the applied state then keeps the previous search. */
  async query(state: ViewState): Promise<QueryReplyDto> {
    const filters: Record<string, string[]> = {};
    for (const [field, values] of state.filters) {
      if (values.size > 0) {
        filters[field] = [...values];
      }
    }
    const reply = await this.client.request<QueryReplyDto>({
      type: "query",
      filters,
      search: state.search,
      range: state.range,
      toggled: [...this.toggled],
    });
    if (reply.compileError === null) {
      const merged = new Map<string, Set<string>>([...state.filters].map(([f, v]) => [f, new Set(v)]));
      for (const t of reply.tokens) {
        const set = merged.get(t.field) ?? new Set<string>();
        set.add(t.value);
        merged.set(t.field, set);
      }
      this.applied = { filters: merged, search: reply.remainingText, range: state.range };
      this.generation = reply.generation;
    }
    this.counts = {
      total: reply.total,
      matched: reply.matched,
      rowCount: reply.rowCount,
      timestamplessExcluded: reply.timestamplessExcluded,
    };
    return reply;
  }

  /** Re-sends the applied state, e.g. after a reconnect or a `reset`. */
  requery(): Promise<QueryReplyDto> {
    return this.query(this.applied);
  }

  /** `null` when the reply belongs to a superseded generation. */
  async window(offset: number, limit: number, anchor?: number): Promise<WindowReplyDto | null> {
    const req: ClientRequest =
      anchor === undefined ? { type: "window", offset, limit } : { type: "window", offset, limit, anchor };
    const reply = await this.client.request<WindowReplyDto>(req);
    if (reply.generation !== this.generation) {
      return null;
    }
    this.counts = { ...this.counts, total: reply.total, matched: reply.matched, rowCount: reply.rowCount };
    return reply;
  }

  async buckets(fullSpan: boolean): Promise<BucketsReplyDto | null> {
    const reply = await this.client.request<BucketsReplyDto>({ type: "buckets", fullSpan });
    return reply.generation === this.generation ? reply : null;
  }

  entry(ordinal: number): Promise<EntryReplyDto> {
    return this.client.request<EntryReplyDto>({ type: "entry", ordinal });
  }

  /** Flips a chain; returns the new row count. */
  async toggleChain(root: number): Promise<number> {
    const reply = await this.client.request<{ rowCount: number }>({ type: "toggle_chain", root });
    if (!this.toggled.delete(root)) {
      this.toggled.add(root);
    }
    this.counts = { ...this.counts, rowCount: reply.rowCount };
    return reply.rowCount;
  }

  stats(): Promise<StatsReplyDto> {
    return this.client.request<StatsReplyDto>({ type: "stats" });
  }

  setFormat(format: string | null, tzOffsetMinutes: number | null): Promise<StatsDto> {
    return this.client.request<StatsDto>({ type: "set_format", format, tzOffsetMinutes });
  }

  /** A `grown` notification: cumulative counts for this connection's view. */
  applyGrown(total: number, matched: number, rowCount: number): void {
    this.counts = { ...this.counts, total, matched, rowCount };
  }

  /** A new source: its ordinals mean nothing to the old toggles. */
  forgetToggles(): void {
    this.toggled.clear();
  }
}
