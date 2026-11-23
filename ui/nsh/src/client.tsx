import { batch, host, platform, signal } from "nsh";
import { DomainStore } from "./domain-store";
import { sanitizeDisplayText } from "./locale";
import type {
  AuthChallenge,
  CommandResult,
  DomainDataMap,
  DomainProjection,
  PreferencesSnapshot,
  RuntimeDomain,
  StateChanged,
} from "./model";

export interface CoreTransport {
  readonly params: Record<string, unknown>;
  call<T = unknown>(
    name: string,
    data?: unknown,
    options?: { timeout?: number },
  ): Promise<T>;
  on<T = unknown>(name: string, listener: (data: T) => void): () => void;
}

class NshHostTransport implements CoreTransport {
  get params(): Record<string, unknown> {
    return (host.params ?? {}) as Record<string, unknown>;
  }

  call<T = unknown>(
    name: string,
    data?: unknown,
    options?: { timeout?: number },
  ): Promise<T> {
    return host.call<T>(name, data, options);
  }

  on<T = unknown>(name: string, listener: (data: T) => void): () => void {
    return host.on<T>(name, listener);
  }
}

const INTERACTIVE_COMMANDS = new Set([
  "vpn.connect",
  "vpn.disconnect",
  "vpn.cancelConnect",
  "vpn.resetState",
  "auth.submit",
  "auth.cancel",
  "profiles.discoverGroups",
  "platform.pickCertFile",
  "platform.exportLog",
  "platform.exportQr",
  "platform.scanQr",
]);

const DEFAULT_PREFS: PreferencesSnapshot = {
  language: "system",
  theme: "system",
  systemLocale: "zh-CN",
  systemColorMode: 1,
};

export class CoreClient {
  readonly transport: CoreTransport;
  readonly domains: DomainStore;
  readonly deviceType: string;
  readonly canScanQr: boolean;
  readonly canGenerateQr: boolean;
  readonly preferences = signal<PreferencesSnapshot>(DEFAULT_PREFS, {
    equals: (prev, next) =>
      prev.language === next.language &&
      prev.theme === next.theme &&
      prev.systemLocale === next.systemLocale &&
      prev.systemColorMode === next.systemColorMode,
  });
  readonly error = signal<string>("");
  readonly warning = signal<string>("");
  readonly notice = signal<string>("");
  readonly pending = signal<number>(0);
  private readonly pendingCommands = signal<Record<string, number>>({});
  private nextRequestId = 1;
  private readonly unsubs: (() => void)[] = [];
  private disposed = false;

  constructor(transport?: CoreTransport) {
    this.transport = transport ?? new NshHostTransport();
    const rawDeviceType = this.transport.params?.deviceType;
    this.deviceType =
      typeof rawDeviceType === "string" && rawDeviceType.length > 0
        ? rawDeviceType
        : platform.desktop
          ? "2in1"
          : "phone";
    const rawCanScan = this.transport.params?.canScanQr;
    this.canScanQr =
      typeof rawCanScan === "boolean" ? rawCanScan : !platform.desktop;
    const rawCanGen = this.transport.params?.canGenerateQr;
    this.canGenerateQr = typeof rawCanGen === "boolean" ? rawCanGen : true;
    const initialPrefs = this.transport.params?.preferences as
      | Partial<PreferencesSnapshot>
      | undefined;
    if (initialPrefs) {
      this.preferences.set({ ...DEFAULT_PREFS, ...initialPrefs });
    }

    this.domains = new DomainStore(
      (generation, domains) =>
        this.transport.call<StateChanged>("core.subscribe", {
          generation,
          domains,
        }),
      (generation, domain) =>
        this.transport.call<DomainProjection<typeof domain>>("core.query", {
          generation,
          domain,
        }),
      (message) => this.pushError(message),
    );

    const initialDomains = this.transport.params?.initialDomains as
      | Partial<DomainDataMap>
      | undefined;
    if (initialDomains) {
      const sanitized: Partial<DomainDataMap> = { ...initialDomains };
      if (sanitized.status) {
        const cleanError = sanitized.status.lastError
          ? sanitizeDisplayText(sanitized.status.lastError)
          : sanitized.status.lastError;
        const cleanAuth: AuthChallenge | null | undefined =
          sanitized.status.pendingAuth
            ? {
                ...sanitized.status.pendingAuth,
                banner: sanitized.status.pendingAuth.banner
                  ? sanitizeDisplayText(sanitized.status.pendingAuth.banner)
                  : sanitized.status.pendingAuth.banner,
                message: sanitized.status.pendingAuth.message
                  ? sanitizeDisplayText(sanitized.status.pendingAuth.message)
                  : sanitized.status.pendingAuth.message,
                error: sanitized.status.pendingAuth.error
                  ? sanitizeDisplayText(sanitized.status.pendingAuth.error)
                  : sanitized.status.pendingAuth.error,
              }
            : sanitized.status.pendingAuth;
        sanitized.status = {
          ...sanitized.status,
          lastError: cleanError,
          pendingAuth: cleanAuth,
        };
      }
      this.domains.seed(sanitized);
    }

    this.unsubs.push(
      this.transport.on<StateChanged>("core.changed", (change) => {
        if (!this.disposed) this.domains.applyChanged(change);
      }),
    );
    this.unsubs.push(
      this.transport.on<PreferencesSnapshot>("core.preferences", (prefs) => {
        if (!this.disposed && prefs) {
          this.preferences.set({ ...DEFAULT_PREFS, ...prefs });
        }
      }),
    );
  }

  isPending(command: string): boolean {
    return (this.pendingCommands()[command] ?? 0) > 0;
  }

  pushError(message: string): void {
    const clean = sanitizeDisplayText(message).trim();
    if (!clean) return;
    // Reset before setting so identical consecutive errors still trigger toasts.
    batch(() => {
      this.error.set("");
      this.error.set(clean);
    });
  }

  pushNotice(message: string): void {
    const clean = sanitizeDisplayText(message).trim();
    if (!clean) return;
    batch(() => {
      this.notice.set("");
      this.notice.set(clean);
    });
  }

  pushWarning(message: string): void {
    const clean = sanitizeDisplayText(message).trim();
    if (!clean) return;
    batch(() => {
      this.warning.set("");
      this.warning.set(clean);
    });
  }

  async run<T = unknown>(
    command: string,
    payload: Record<string, unknown> = {},
    successNotice?: string,
  ): Promise<T | undefined> {
    if (this.disposed) return undefined;
    const id = this.nextRequestId++;
    this.bumpPending(command, 1);
    const timeout = INTERACTIVE_COMMANDS.has(command) ? 0 : undefined;
    try {
      const result = await this.transport.call<CommandResult<T>>(
        "core.command",
        { id, command, payload },
        timeout !== undefined ? { timeout } : undefined,
      );
      if (this.disposed) return undefined;
      if (!result || !result.ok) {
        this.pushError(result?.error || "Operation failed");
        return undefined;
      }
      if (command === "preferences.set" && result.data) {
        this.preferences.set({
          ...DEFAULT_PREFS,
          ...(result.data as Partial<PreferencesSnapshot>),
        });
      }
      if (result.barrier) {
        await this.domains.waitFor(result.barrier);
        if (this.disposed) return undefined;
      }
      if (successNotice) {
        this.pushNotice(successNotice);
      }
      return result.data;
    } catch (err) {
      if (!this.disposed) {
        this.pushError(err instanceof Error ? err.message : String(err));
      }
      return undefined;
    } finally {
      this.bumpPending(command, -1);
    }
  }

  async pickCertFile(
    kind: "certificate" | "private_key" | "ca_certificate",
  ): Promise<string | undefined> {
    if (this.disposed) return undefined;
    this.bumpPending("platform.pickCertFile", 1);
    try {
      const result = await this.transport.call<{ path?: string }>(
        "platform.pickCertFile",
        { kind },
        { timeout: 0 },
      );
      return result?.path || "";
    } catch (err) {
      if (!this.disposed) {
        this.pushError(err instanceof Error ? err.message : String(err));
      }
      return undefined;
    } finally {
      this.bumpPending("platform.pickCertFile", -1);
    }
  }

  async exportLog(
    fileName: string,
    content?: string,
    successNotice?: string,
  ): Promise<boolean> {
    if (this.disposed) return false;
    this.bumpPending("platform.exportLog", 1);
    try {
      await this.transport.call(
        "platform.exportLog",
        { fileName, content },
        { timeout: 0 },
      );
      if (successNotice && !this.disposed) {
        this.pushNotice(successNotice);
      }
      return true;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (!this.disposed && !msg.toLowerCase().includes("cancel")) {
        this.pushError(msg);
      }
      return false;
    } finally {
      this.bumpPending("platform.exportLog", -1);
    }
  }

  async openUrl(url: string): Promise<void> {
    if (this.disposed) return;
    try {
      await this.transport.call("platform.openUrl", { url });
    } catch (err) {
      if (!this.disposed) {
        this.pushError(err instanceof Error ? err.message : String(err));
      }
    }
  }

  async generateQr(
    text: string,
  ): Promise<{ source: string; pngBase64: string } | undefined> {
    if (this.disposed) return undefined;
    this.bumpPending("platform.qr", 1);
    try {
      const result = await this.transport.call<{
        source?: string;
        pngBase64?: string;
      }>("platform.qr", { text });
      if (result?.source && result?.pngBase64) {
        return { source: result.source, pngBase64: result.pngBase64 };
      }
      return undefined;
    } catch (err) {
      if (!this.disposed) {
        this.pushError(err instanceof Error ? err.message : String(err));
      }
      return undefined;
    } finally {
      this.bumpPending("platform.qr", -1);
    }
  }

  async exportQr(
    name: string,
    pngBase64: string,
    successNotice?: string,
  ): Promise<boolean> {
    if (this.disposed) return false;
    this.bumpPending("platform.exportQr", 1);
    try {
      await this.transport.call(
        "platform.exportQr",
        { name, pngBase64 },
        { timeout: 0 },
      );
      if (successNotice && !this.disposed) {
        this.pushNotice(successNotice);
      }
      return true;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (!this.disposed && !msg.toLowerCase().includes("cancel")) {
        this.pushError(msg);
      }
      return false;
    } finally {
      this.bumpPending("platform.exportQr", -1);
    }
  }

  async scanQr(): Promise<string | undefined> {
    if (this.disposed) return undefined;
    this.bumpPending("platform.scanQr", 1);
    try {
      const result = await this.transport.call<{ content?: string }>(
        "platform.scanQr",
        {},
        { timeout: 0 },
      );
      return result?.content?.trim() || "";
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (!this.disposed && !msg.toLowerCase().includes("cancel")) {
        this.pushError(msg);
      }
      return undefined;
    } finally {
      this.bumpPending("platform.scanQr", -1);
    }
  }

  setInterests(domains: RuntimeDomain[]): Promise<void> {
    return this.domains.setInterests(domains);
  }

  dispose(): void {
    this.disposed = true;
    for (const off of this.unsubs) {
      try {
        off();
      } catch {
        // ignore
      }
    }
    this.unsubs.length = 0;
    this.domains.dispose();
  }

  private bumpPending(command: string, delta: number): void {
    batch(() => {
      this.pending.set(Math.max(0, this.pending() + delta));
      this.pendingCommands.update((map) => {
        const next = Math.max(0, (map[command] ?? 0) + delta);
        const copy = { ...map };
        if (next === 0) delete copy[command];
        else copy[command] = next;
        return copy;
      });
    });
  }
}
