import { batch, signal } from "nsh";
import type {
  DomainDataMap,
  DomainProjection,
  DomainRevision,
  RuntimeDomain,
  StateBarrier,
  StateChanged,
} from "./model";

type PendingBarrier = {
  generation: number;
  required: DomainRevision[];
  resolve: () => void;
  timer: ReturnType<typeof setTimeout>;
};

export type DomainQueryFn = <D extends RuntimeDomain>(
  generation: number,
  domain: D,
) => Promise<DomainProjection<D>>;

export type DomainSubscribeFn = (
  generation: number,
  domains: { domain: RuntimeDomain }[],
) => Promise<StateChanged>;

export class DomainStore {
  private readonly store = {
    config: signal<DomainDataMap["config"] | undefined>(undefined),
    status: signal<DomainDataMap["status"] | undefined>(undefined),
    profiles: signal<DomainDataMap["profiles"] | undefined>(undefined),
    statistics: signal<DomainDataMap["statistics"] | undefined>(undefined),
    logs: signal<DomainDataMap["logs"] | undefined>(undefined),
    about: signal<DomainDataMap["about"] | undefined>(undefined),
  };

  private readonly loadingMap = {
    config: signal(false),
    status: signal(false),
    profiles: signal(false),
    statistics: signal(false),
    logs: signal(false),
    about: signal(false),
  };

  private readonly revisions: Partial<Record<RuntimeDomain, number>> = {};
  private readonly targetRevisions: Partial<Record<RuntimeDomain, number>> = {};
  private readonly inFlight = new Set<RuntimeDomain>();
  private readonly rerunNeeded = new Set<RuntimeDomain>();
  private readonly barriers = new Set<PendingBarrier>();
  private generation = 0;
  private activeSet = new Set<RuntimeDomain>(["config", "status"]);
  private disposed = false;

  constructor(
    private readonly subscribeFn: DomainSubscribeFn,
    private readonly queryFn: DomainQueryFn,
    private readonly onError: (message: string) => void,
  ) {}

  data<D extends RuntimeDomain>(domain: D): DomainDataMap[D] | undefined {
    return this.store[domain]() as DomainDataMap[D] | undefined;
  }

  has(domains: RuntimeDomain[]): boolean {
    for (const domain of domains) {
      if (this.store[domain]() === undefined) return false;
    }
    return true;
  }

  seed(initial: Partial<DomainDataMap>): void {
    batch(() => {
      for (const domain of Object.keys(initial) as RuntimeDomain[]) {
        const value = initial[domain];
        if (value === undefined) continue;
        if (this.store[domain]() !== undefined) continue;
        (this.store[domain] as { set(v: unknown): void }).set(value);
      }
    });
  }

  loading(domain: RuntimeDomain): boolean {
    return this.loadingMap[domain]();
  }

  fresh(domains: RuntimeDomain[]): boolean {
    for (const domain of domains) {
      if (this.store[domain]() === undefined) return false;
      const target = this.targetRevisions[domain] ?? 0;
      const seen = this.revisions[domain] ?? 0;
      if (target > seen) return false;
    }
    return true;
  }

  async setInterests(domains: RuntimeDomain[]): Promise<void> {
    if (this.disposed) return;
    const next = new Set<RuntimeDomain>(["config", "status", ...domains]);
    if (
      this.generation > 0 &&
      next.size === this.activeSet.size &&
      [...next].every((d) => this.activeSet.has(d))
    ) {
      return;
    }
    const gen = ++this.generation;
    this.activeSet = next;
    this.inFlight.clear();
    this.rerunNeeded.clear();
    this.checkBarriers();
    try {
      const changed = await this.subscribeFn(
        gen,
        [...next].map((domain) => ({ domain })),
      );
      if (this.disposed || gen !== this.generation) return;
      this.applyChanged(changed);
    } catch (err) {
      if (!this.disposed && gen === this.generation) {
        this.onError(err instanceof Error ? err.message : String(err));
      }
    }
  }

  applyChanged(change: StateChanged): void {
    if (this.disposed || !change) return;
    if (change.generation !== 0 && change.generation !== this.generation) return;
    for (const item of change.changes) {
      this.scheduleFetch(item);
    }
  }

  refresh(domains?: RuntimeDomain[]): void {
    if (this.disposed || this.generation === 0) return;
    const targets = domains ?? [...this.activeSet];
    for (const domain of targets) {
      if (this.activeSet.has(domain)) {
        const nextRev = (this.targetRevisions[domain] ?? 0) + 1;
        this.scheduleFetch({ domain, revision: nextRev });
      }
    }
  }

  waitFor(barrier: StateBarrier, timeoutMs = 4000): Promise<void> {
    if (this.disposed || !barrier || !Array.isArray(barrier.required)) {
      return Promise.resolve();
    }
    if (barrier.generation !== 0 && barrier.generation !== this.generation) {
      return Promise.resolve();
    }
    const relevant = barrier.required.filter((item) =>
      this.activeSet.has(item.domain),
    );
    if (relevant.length === 0) return Promise.resolve();
    for (const item of relevant) {
      if ((this.revisions[item.domain] ?? 0) < item.revision) {
        this.scheduleFetch(item);
      }
    }
    if (this.isBarrierSatisfied(relevant)) {
      return Promise.resolve();
    }
    return new Promise<void>((resolve) => {
      const pending: PendingBarrier = {
        generation: barrier.generation,
        required: relevant,
        resolve,
        timer: setTimeout(() => {
          this.barriers.delete(pending);
          resolve();
        }, timeoutMs),
      };
      this.barriers.add(pending);
      this.checkBarriers();
    });
  }

  dispose(): void {
    this.disposed = true;
    this.inFlight.clear();
    this.rerunNeeded.clear();
    for (const pending of this.barriers) {
      clearTimeout(pending.timer);
      pending.resolve();
    }
    this.barriers.clear();
  }

  private isBarrierSatisfied(required: DomainRevision[]): boolean {
    for (const item of required) {
      if (!this.activeSet.has(item.domain)) continue;
      if ((this.revisions[item.domain] ?? 0) < item.revision) return false;
    }
    return true;
  }

  private checkBarriers(): void {
    if (this.barriers.size === 0) return;
    for (const pending of [...this.barriers]) {
      if (
        this.disposed ||
        (pending.generation !== 0 && pending.generation !== this.generation) ||
        this.isBarrierSatisfied(pending.required)
      ) {
        clearTimeout(pending.timer);
        this.barriers.delete(pending);
        pending.resolve();
      }
    }
  }

  private scheduleFetch(item: DomainRevision): void {
    const { domain, revision } = item;
    if (!this.activeSet.has(domain)) return;
    const prevTarget = this.targetRevisions[domain] ?? 0;
    const newer = revision > prevTarget;
    if (newer) {
      this.targetRevisions[domain] = revision;
    }
    if (this.inFlight.has(domain)) {
      if (newer) {
        this.rerunNeeded.add(domain);
      }
      return;
    }
    void this.fetchDomain(this.generation, domain);
  }

  private async fetchDomain<D extends RuntimeDomain>(
    gen: number,
    domain: D,
  ): Promise<void> {
    if (this.disposed || gen !== this.generation || !this.activeSet.has(domain)) {
      return;
    }
    this.inFlight.add(domain);
    this.loadingMap[domain].set(true);
    try {
      const projection = await this.queryFn(gen, domain);
      if (this.disposed || gen !== this.generation) return;
      batch(() => {
        const previous = this.store[domain]();
        const unchanged =
          previous !== undefined &&
          JSON.stringify(previous) === JSON.stringify(projection.data);
        if (!unchanged) {
          (this.store[domain] as { set(v: DomainDataMap[D]): void }).set(
            projection.data,
          );
        }
        this.revisions[domain] = Math.max(
          this.revisions[domain] ?? 0,
          projection.revision,
        );
        this.loadingMap[domain].set(false);
      });
      this.checkBarriers();
    } catch (err) {
      if (this.disposed || gen !== this.generation) return;
      this.loadingMap[domain].set(false);
      const msg = err instanceof Error ? err.message : String(err);
      if (
        !msg.includes("superseded") &&
        !msg.includes("already in flight") &&
        !msg.includes("closed")
      ) {
        this.onError(msg);
      }
    } finally {
      if (gen === this.generation) {
        this.inFlight.delete(domain);
        if (this.rerunNeeded.delete(domain)) {
          void this.fetchDomain(gen, domain);
        }
      }
    }
  }
}
