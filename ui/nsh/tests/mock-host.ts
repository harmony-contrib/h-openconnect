import type { App, HostOptions } from "nsh/test";
import {
  newDraftProfile,
  type AboutDomainData,
  type AuthChallenge,
  type CommandResult,
  type ConfigDomainData,
  type ConnectionLifecycle,
  type ConnectionProfile,
  type DiagnosticEntry,
  type DomainDataMap,
  type DomainProjection,
  type LogArchiveSummary,
  type LogsDomainData,
  type NetworkSnapshot,
  type PreferencesSnapshot,
  type ProfilesDomainData,
  type RuntimeDomain,
  type SessionStats,
  type StateChanged,
  type StatisticsDomainData,
  type StatusDomainData,
  type TrafficSample,
} from "../src/model";

export interface RecordedCommand {
  command: string;
  payload: Record<string, unknown>;
}

function defaultProfile(
  overrides: Partial<ConnectionProfile> = {},
): ConnectionProfile {
  return {
    ...newDraftProfile(),
    id: "hq-gw",
    name: "HQ Gateway",
    server: "gw.example.com",
    group: "Employees",
    username: "alice",
    password: "secret",
    externalBrowserAuth: true,
    mtu: 1400,
    dpdSeconds: 30,
    favorite: true,
    ...overrides,
  };
}

function makeChallenge(): AuthChallenge {
  return {
    id: 1,
    round: 1,
    banner: "Corporate Access",
    message: "Enter the verification code from your authenticator app.",
    error: null,
    formId: "mfa",
    method: "POST",
    fields: [
      {
        key: { formId: "mfa", optionIndex: 0, optionDigest: "otp-0" },
        name: "otp",
        label: "Verification Code",
        kind: "text",
        value: "",
        choices: [],
        authGroup: false,
        secondAuth: true,
        required: true,
      },
    ],
  };
}

export class MockHOpenHost {
  readonly commands: RecordedCommand[] = [];
  readonly openedUrls: string[] = [];
  readonly exportedLogs: { fileName: string; content?: string }[] = [];
  readonly exportedQrs: { name: string; pngBase64: string }[] = [];

  private appRef: App | null = null;
  private generation = 0;
  private subscribed = new Set<RuntimeDomain>(["config", "status"]);
  private revisions: Record<RuntimeDomain, number> = {
    config: 1,
    status: 1,
    profiles: 1,
    statistics: 1,
    logs: 1,
    about: 1,
  };

  preferences: PreferencesSnapshot;
  lifecycle: ConnectionLifecycle = "disconnected";
  activeConnectionId: string | null = "hq-gw";
  pendingAuth: AuthChallenge | null = null;
  lastError: string | null = null;
  requireMfaOnConnect = true;
  recording = false;

  profiles: ConnectionProfile[] = [
    defaultProfile(),
    defaultProfile({
      id: "lab-sg",
      name: "Singapore Lab",
      server: "sg-lab.example.com:8443",
      group: "Engineering",
      favorite: false,
    }),
  ];

  stats: SessionStats = {
    bytesSent: 0,
    bytesReceived: 0,
    packetsSent: 0,
    packetsReceived: 0,
    connectedSeconds: 0,
    assignedIp: "",
    gateway: "",
    mtu: 1400,
  };

  trafficHistory: TrafficSample[] = [];
  network: NetworkSnapshot = {
    address: null,
    netmask: null,
    addressV6: null,
    netmaskV6: null,
    gateway: null,
    dns: [],
    mtu: 1400,
    routes: [],
    splitExcludes: [],
    domain: null,
    splitDns: [],
  };

  logs: DiagnosticEntry[] = [
    {
      timestamp: "2026-11-23T10:00:01Z",
      level: "info",
      message: "Session engine ready",
    },
    {
      timestamp: "2026-11-23T10:00:05Z",
      level: "warn",
      message: "DTLS handshake fallback probe scheduled",
    },
    {
      timestamp: "2026-11-23T10:00:09Z",
      level: "error",
      message: "Gateway rejected expired token",
    },
  ];

  archives: LogArchiveSummary[] = [
    {
      fileName: "hopenconnect-2026-11-22.log",
      date: "2026-11-22",
      bytes: 4096,
      updatedAt: "2026-11-22T18:30:00Z",
      active: false,
    },
  ];

  constructor(initialPrefs: Partial<PreferencesSnapshot> = {}) {
    this.preferences = {
      language: "system",
      theme: "system",
      systemLocale: "zh-CN",
      systemColorMode: 1,
      ...initialPrefs,
    };
  }

  attachApp(app: App): void {
    this.appRef = app;
  }

  hostOptions(
    options: {
      seedInitialDomains?: boolean;
      deviceType?: string;
      canScanQr?: boolean;
      canGenerateQr?: boolean;
    } = {},
  ): HostOptions {
    const initialDomains = options.seedInitialDomains
      ? {
          config: this.queryDomain(0, "config").data,
          status: this.queryDomain(0, "status").data,
          profiles: this.queryDomain(0, "profiles").data,
          statistics: this.queryDomain(0, "statistics").data,
          about: this.queryDomain(0, "about").data,
        }
      : undefined;
    return {
      params: {
        preferences: this.preferences,
        ...(options.deviceType !== undefined
          ? { deviceType: options.deviceType }
          : {}),
        ...(options.canScanQr !== undefined
          ? { canScanQr: options.canScanQr }
          : {}),
        ...(options.canGenerateQr !== undefined
          ? { canGenerateQr: options.canGenerateQr }
          : {}),
        ...(initialDomains ? { initialDomains } : {}),
      },
      handlers: {
        "core.subscribe": (raw: unknown) => {
          const req = raw as {
            generation: number;
            domains: { domain: RuntimeDomain }[];
          };
          this.generation = req.generation;
          this.subscribed = new Set(req.domains.map((d) => d.domain));
          const changes = [...this.subscribed].map((domain) => ({
            domain,
            revision: this.revisions[domain],
          }));
          const reply: StateChanged = {
            generation: this.generation,
            revision: this.maxRevision(),
            changes,
          };
          return reply;
        },
        "core.query": (raw: unknown) => {
          const req = raw as { generation: number; domain: RuntimeDomain };
          return this.queryDomain(req.generation, req.domain);
        },
        "core.command": (raw: unknown) => {
          const req = raw as {
            id: number;
            command: string;
            payload?: Record<string, unknown>;
          };
          return this.handleCommand(req.id, req.command, req.payload ?? {});
        },
        "platform.pickCertFile": (raw: unknown) => {
          const req = raw as { kind: string };
          return { path: `/data/storage/certs/picked-${req.kind}.pem` };
        },
        "platform.exportLog": (raw: unknown) => {
          const req = raw as { fileName: string; content?: string };
          this.exportedLogs.push(req);
          return { exported: true };
        },
        "platform.openUrl": (raw: unknown) => {
          const req = raw as { url: string };
          this.openedUrls.push(req.url);
          return { opened: true };
        },
        "platform.qr": () => ({
          source:
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
          pngBase64:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
        }),
        "platform.exportQr": (raw: unknown) => {
          const req = raw as { name: string; pngBase64: string };
          this.exportedQrs.push(req);
          return { ok: true };
        },
        "platform.scanQr": () => ({
          content: JSON.stringify({
            format: "h-openconnect.connection",
            version: 1,
            profile: {
              ...defaultProfile({
                id: "",
                name: "Scanned Branch",
                server: "branch.example.com",
                favorite: false,
              }),
            },
          }),
        }),
      },
    };
  }

  private maxRevision(): number {
    return Math.max(...Object.values(this.revisions));
  }

  private emitChanged(domains: RuntimeDomain[]): StateChanged["changes"] {
    for (const d of domains) {
      this.revisions[d] += 1;
    }
    const relevant = domains.filter((d) => this.subscribed.has(d));
    const changes = relevant.map((domain) => ({
      domain,
      revision: this.revisions[domain],
    }));
    if (!relevant.length || !this.appRef) return changes;
    const change: StateChanged = {
      generation: this.generation,
      revision: this.maxRevision(),
      changes,
    };
    void this.appRef.host.send("core.changed", change);
    return changes;
  }

  private barrierFor(domains: RuntimeDomain[]) {
    return {
      generation: this.generation,
      required: this.emitChanged(domains),
    };
  }

  private queryDomain<D extends RuntimeDomain>(
    generation: number,
    domain: D,
  ): DomainProjection<D> {
    let data: DomainDataMap[RuntimeDomain];
    switch (domain) {
      case "config": {
        const d: ConfigDomainData = {
          activeConnectionId: this.activeConnectionId,
          ...this.preferences,
          sdkReady: true,
          backend: "anyconnect-rs",
        };
        data = d;
        break;
      }
      case "status": {
        const d: StatusDomainData = {
          lifecycle: this.lifecycle,
          vpnSessionId: this.lifecycle === "connected" ? "sess-1" : "",
          lastError: this.lastError,
          sdkReady: true,
          backend: "anyconnect-rs",
          pendingAuth: this.pendingAuth,
          operationPending: false,
        };
        data = d;
        break;
      }
      case "profiles": {
        const d: ProfilesDomainData = {
          activeConnectionId: this.activeConnectionId,
          profiles: this.profiles.map((p) => ({ ...p })),
        };
        data = d;
        break;
      }
      case "statistics": {
        const d: StatisticsDomainData = {
          lifecycle: this.lifecycle,
          stats: { ...this.stats },
          network: { ...this.network },
          trafficHistory: this.trafficHistory.map((s) => ({ ...s })),
        };
        data = d;
        break;
      }
      case "logs": {
        const d: LogsDomainData = {
          entries: this.logs.map((e) => ({ ...e })),
          recording: {
            enabled: this.recording,
            activeFile: this.recording ? "hopenconnect-2026-11-23.log" : null,
            archives: this.archives.map((a) => ({ ...a })),
          },
        };
        data = d;
        break;
      }
      case "about": {
        const d: AboutDomainData = {
          appVersion: "2.0.0",
          openconnectVersion: "OpenConnect 9.12",
          backend: "anyconnect-rs",
          sdkReady: true,
        };
        data = d;
        break;
      }
    }
    return {
      generation,
      domain,
      revision: this.revisions[domain],
      data: data as DomainDataMap[D],
    };
  }

  private transitionToConnected(): void {
    this.lifecycle = "connected";
    this.pendingAuth = null;
    this.lastError = null;
    this.stats = {
      bytesReceived: 2_450_000,
      bytesSent: 640_000,
      packetsReceived: 3200,
      packetsSent: 1400,
      connectedSeconds: 95,
      assignedIp: "10.24.0.15",
      gateway: "gw.example.com",
      mtu: 1400,
    };
    this.network = {
      address: "10.24.0.15",
      netmask: "255.255.255.0",
      addressV6: "fd00:24::15",
      netmaskV6: "64",
      gateway: "gw.example.com",
      dns: ["10.24.0.1", "10.24.0.2"],
      mtu: 1400,
      routes: ["10.0.0.0/8", "172.16.0.0/12"],
      splitExcludes: ["192.168.0.0/16"],
      domain: "corp.example.com",
      splitDns: ["corp.example.com"],
    };
    this.trafficHistory = [
      { rxBytesPerSec: 64_000, txBytesPerSec: 21_000 },
      { rxBytesPerSec: 96_000, txBytesPerSec: 34_000 },
      { rxBytesPerSec: 128_000, txBytesPerSec: 42_000 },
    ];
    this.logs.push({
      timestamp: "2026-11-23T10:01:00Z",
      level: "info",
      message: "Tunnel established with gw.example.com",
    });
  }

  private handleCommand(
    id: number,
    command: string,
    payload: Record<string, unknown>,
  ): CommandResult {
    this.commands.push({ command, payload });

    switch (command) {
      case "preferences.set": {
        if (typeof payload.language === "string") {
          this.preferences.language =
            payload.language as PreferencesSnapshot["language"];
        }
        if (typeof payload.theme === "string") {
          this.preferences.theme = payload.theme as PreferencesSnapshot["theme"];
        }
        this.emitChanged(["config"]);
        return { id, ok: true, data: { ...this.preferences } };
      }

      case "vpn.connect": {
        if (typeof payload.profileId === "string" && payload.profileId) {
          this.activeConnectionId = payload.profileId;
        }
        if (this.requireMfaOnConnect) {
          this.lifecycle = "authenticating";
          this.pendingAuth = makeChallenge();
          this.emitChanged(["status"]);
        } else {
          this.transitionToConnected();
          this.emitChanged(["status", "statistics", "logs"]);
        }
        return { id, ok: true, data: { lifecycle: this.lifecycle } };
      }

      case "auth.submit": {
        this.transitionToConnected();
        this.emitChanged(["status", "statistics", "logs"]);
        return { id, ok: true, data: { lifecycle: this.lifecycle } };
      }

      case "auth.cancel": {
        this.lifecycle = "disconnected";
        this.pendingAuth = null;
        this.emitChanged(["status"]);
        return { id, ok: true, data: { lifecycle: this.lifecycle } };
      }

      case "vpn.disconnect":
      case "vpn.resetState": {
        this.lifecycle = "disconnected";
        this.pendingAuth = null;
        this.lastError = null;
        this.stats = {
          bytesSent: 0,
          bytesReceived: 0,
          packetsSent: 0,
          packetsReceived: 0,
          connectedSeconds: 0,
          assignedIp: "",
          gateway: "",
          mtu: 1400,
        };
        this.emitChanged(["status", "statistics"]);
        return { id, ok: true, data: { lifecycle: this.lifecycle } };
      }

      case "vpn.syncState": {
        this.emitChanged(["status", "statistics"]);
        return { id, ok: true, data: { lifecycle: this.lifecycle } };
      }

      case "vpn.setForceGlobal": {
        const enabled = Boolean(payload.enabled);
        const active = this.profiles.find(
          (p) => p.id === this.activeConnectionId,
        );
        if (active) active.forceGlobal = enabled;
        this.emitChanged(["status", "profiles"]);
        return { id, ok: true, data: { enabled } };
      }

      case "profiles.select": {
        const targetId = String(payload.id ?? "");
        if (this.profiles.some((p) => p.id === targetId)) {
          this.activeConnectionId = targetId;
          return {
            id,
            ok: true,
            data: { id: targetId },
            barrier: this.barrierFor(["config", "status", "profiles"]),
          };
        }
        return { id, ok: false, error: "Profile not found" };
      }

      case "profiles.setForceGlobal": {
        const enabled = Boolean(payload.forceGlobal);
        const active = this.profiles.find(
          (p) => p.id === this.activeConnectionId,
        );
        if (active) active.forceGlobal = enabled;
        return {
          id,
          ok: true,
          data: { forceGlobal: enabled },
          barrier: this.barrierFor(["status", "profiles"]),
        };
      }

      case "profiles.save": {
        const incoming = payload.profile as ConnectionProfile;
        if (!incoming?.name?.trim() || !incoming?.server?.trim()) {
          return { id, ok: false, error: "Name and server are required" };
        }
        const profileId =
          incoming.id?.trim() || `profile-${this.profiles.length + 1}`;
        const saved: ConnectionProfile = { ...incoming, id: profileId };
        const idx = this.profiles.findIndex((p) => p.id === profileId);
        if (idx >= 0) this.profiles[idx] = saved;
        else this.profiles.push(saved);
        if (!this.activeConnectionId) this.activeConnectionId = profileId;
        this.emitChanged(["config", "profiles", "status"]);
        return { id, ok: true, data: saved };
      }

      case "profiles.delete": {
        const targetId = String(payload.id ?? "");
        this.profiles = this.profiles.filter((p) => p.id !== targetId);
        if (this.activeConnectionId === targetId) {
          this.activeConnectionId = this.profiles[0]?.id ?? null;
        }
        this.emitChanged(["config", "profiles", "status"]);
        return { id, ok: true, data: { deleted: targetId } };
      }

      case "profiles.toggleFavorite": {
        const targetId = String(payload.id ?? "");
        const target = this.profiles.find((p) => p.id === targetId);
        if (!target) return { id, ok: false, error: "Profile not found" };
        target.favorite = !target.favorite;
        this.emitChanged(["profiles", "status"]);
        return { id, ok: true, data: { id: targetId, favorite: target.favorite } };
      }

      case "profiles.discoverGroups": {
        return {
          id,
          ok: true,
          data: {
            selected: "Employees",
            groups: [
              { name: "Employees", label: "Employees" },
              { name: "Engineering", label: "Engineering" },
            ],
          },
        };
      }

      case "profiles.encodeQr": {
        const targetId = String(payload.id ?? "");
        const target = this.profiles.find((p) => p.id === targetId);
        if (!target) return { id, ok: false, error: "Profile not found" };
        return {
          id,
          ok: true,
          data: {
            name: target.name,
            payload: JSON.stringify({
              format: "h-openconnect.connection",
              version: 1,
              profile: { ...target, id: "" },
            }),
          },
        };
      }

      case "profiles.importQr": {
        const raw = String(payload.content ?? "");
        const parsed = JSON.parse(raw) as { profile: ConnectionProfile };
        const newId = `conn-qr-${this.profiles.length + 1}`;
        const saved: ConnectionProfile = { ...parsed.profile, id: newId };
        this.profiles.push(saved);
        this.emitChanged(["profiles", "status"]);
        return { id, ok: true, data: { id: newId, name: saved.name } };
      }

      case "logs.setRecording": {
        this.recording = Boolean(payload.enabled);
        const activeName = "hopenconnect-2026-11-23.log";
        const existing = this.archives.find((a) => a.fileName === activeName);
        if (this.recording) {
          if (existing) existing.active = true;
          else {
            this.archives.unshift({
              fileName: activeName,
              date: "2026-11-23",
              bytes: 1024,
              updatedAt: "2026-11-23T10:05:00Z",
              active: true,
            });
          }
        } else if (existing) {
          existing.active = false;
        }
        this.emitChanged(["logs", "status"]);
        return { id, ok: true, data: { recording: this.recording } };
      }

      case "logs.clear": {
        this.logs = [];
        this.emitChanged(["logs"]);
        return { id, ok: true, data: { cleared: true } };
      }

      case "logs.readArchive": {
        const fileName = String(payload.fileName ?? "");
        return {
          id,
          ok: true,
          data: {
            fileName,
            content: `2026-11-22T09:00:00Z INFO archived ${fileName}\n`,
          },
        };
      }

      case "logs.deleteArchive": {
        const fileName = String(payload.fileName ?? "");
        const item = this.archives.find((a) => a.fileName === fileName);
        if (item?.active) {
          return {
            id,
            ok: false,
            error: "Stop recording before deleting the active archive",
          };
        }
        this.archives = this.archives.filter((a) => a.fileName !== fileName);
        this.emitChanged(["logs"]);
        return { id, ok: true, data: { deleted: fileName } };
      }

      default:
        return { id, ok: false, error: `Unknown command: ${command}` };
    }
  }
}
