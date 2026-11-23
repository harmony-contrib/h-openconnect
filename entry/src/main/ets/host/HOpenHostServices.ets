import { ConfigurationConstant, common } from '@kit.AbilityKit';
import { UIContext } from '@kit.ArkUI';
import { deviceInfo } from '@kit.BasicServicesKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { NativeAuthSession, NshHostReply, NshSurface, SystemReply } from '@nsh/harmonyos';
import window from '@ohos.window';
import hopenconnectUi, { UiCommandSession } from 'libhopenconnect_ui.so';
import { describeError } from '../common/Errors';
import { CertFileService } from './CertFileService';
import { ExportService } from './ExportService';
import { QrService } from './QrService';
import { VpnService } from './VpnService';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenHost';

interface UiRequest {
  id: number;
  command: string;
  payload: Object;
}

interface UiEffect {
  kind: string;
  optionsJson: string;
}

interface DomainRevision {
  domain: string;
  revision: number;
}

interface StateChanged {
  generation: number;
  revision: number;
  changes: DomainRevision[];
}

interface StateBarrier {
  generation: number;
  required: DomainRevision[];
}

interface StateObservation {
  change: StateChanged;
  barrier: StateBarrier;
}

interface UiResult {
  id: number;
  ok: boolean;
  data?: Object | null;
  error?: string;
  effect?: UiEffect;
  barrier?: StateBarrier;
}

interface UiPreferences {
  language: string;
  theme: string;
  systemLocale: string;
  systemColorMode: number;
}

interface SessionSnapshotSeed {
  lifecycle?: string;
  activeConnectionId?: string | null;
  connections?: Object[];
  stats?: Object;
  network?: Object;
  lastError?: string | null;
  appVersion?: string;
  sdkReady?: boolean;
  anyconnectVersion?: string | null;
  backend?: string;
  pendingAuth?: Object | null;
}

interface InitialConfigDomain {
  activeConnectionId: string | null;
  language: string;
  theme: string;
  systemLocale: string;
  systemColorMode: number;
  sdkReady: boolean;
  backend: string;
}

interface InitialStatusDomain {
  lifecycle: string;
  vpnSessionId: string;
  lastError: string | null;
  sdkReady: boolean;
  backend: string;
  pendingAuth: Object | null;
  operationPending: boolean;
}

interface InitialProfilesDomain {
  activeConnectionId: string | null;
  profiles: Object[];
}

interface InitialStatisticsDomain {
  lifecycle: string;
  stats: Object;
  network: Object;
  trafficHistory: Object[];
}

interface InitialAboutDomain {
  appVersion: string;
  openconnectVersion: string | null;
  backend: string;
  sdkReady: boolean;
}

interface InitialDomains {
  config: InitialConfigDomain;
  status: InitialStatusDomain;
  profiles: InitialProfilesDomain;
  statistics?: InitialStatisticsDomain;
  about: InitialAboutDomain;
}

interface LaunchParams {
  preferences: UiPreferences;
  deviceType: string;
  canScanQr: boolean;
  canGenerateQr: boolean;
  initialDomains?: InitialDomains;
}

interface PickCertArgs {
  kind?: string;
}

interface PickCertResult {
  path: string;
}

interface ExportLogArgs {
  fileName?: string;
  content?: string;
}

interface OpenUrlArgs {
  url?: string;
}

interface QrGenerateArgs {
  text?: string;
}

interface QrExportArgs {
  name?: string;
  pngBase64?: string;
}

interface ScanQrResult {
  content: string;
}

interface TickOutcome {
  lifecycle: string;
  shouldReconnect: boolean;
  activeConnectionId?: string | null;
}

interface BrowserOpenRequest {
  requestId: string;
  attemptId: string;
  uri: string;
  requestedAtMs: number;
}

interface ExternalBrowserBridge {
  openUrl: (uri: string) => boolean;
}

interface SimpleOk {
  ok: boolean;
}

/**
 * First-class `nsh/host` service controller owned by `EntryAbility`.
 *
 * Bridges `ui/nsh` (`host.call` / `host.on` / `host.params`) to
 * `hopenconnect_app` (`UiCommandSession`), HarmonyOS system capabilities
 * (`VpnService`, `CertFileService`, `ExportService`), and upstream
 * `@nsh/harmonyos` `NativeAuthSession` for web SSO authentication.
 */
export class HOpenHostServices {
  readonly vpn: VpnService = new VpnService();
  private readonly context: common.UIAbilityContext;
  private readonly certFiles: CertFileService;
  private readonly exporter: ExportService;
  private readonly qr: QrService = new QrService();
  private readonly session: UiCommandSession = new UiCommandSession();
  private surface?: NshSurface;
  private mainWindow?: window.Window;
  private authSession?: NativeAuthSession;
  private activeAuthKey: number = 0;
  private nextAuthKey: number = 1;
  private closingAuthProgrammatically: boolean = false;
  private closed: boolean = false;
  private surfaceGeneration: number = 0;
  private publishedPreferences: string = '';
  private requestedBarDark?: boolean;
  private barUpdates: Promise<void> = Promise.resolve();
  private connectedTimer: number = -1;
  private connectInFlight: boolean = false;
  private autoReconnectInFlight: boolean = false;
  private autoReconnectId: number = 900000;

  constructor(context: common.UIAbilityContext) {
    this.context = context;
    this.certFiles = new CertFileService(context);
    this.exporter = new ExportService(context);
    this.vpn.attachHost();
  }

  attachMainWindow(mainWindow: window.Window): void {
    this.mainWindow = mainWindow;
    this.requestedBarDark = undefined;
    const prefs = this.readPreferencesSafe();
    if (prefs) {
      this.applyPreferences(prefs);
    }
  }

  attachSurface(surface: NshSurface, uiContext: UIContext): void {
    this.detachSurface();
    if (this.closed) {
      return;
    }
    const generation = ++this.surfaceGeneration;
    this.surface = surface;
    this.publishedPreferences = '';

    this.authSession = new NativeAuthSession(uiContext);
    this.syncAuthSessionColorMode();

    const browserBridge: ExternalBrowserBridge = {
      openUrl: (uri: string): boolean => this.openWebAuthSession(uri),
    };
    try {
      hopenconnectUi.registerExternalBrowserHandler(browserBridge);
    } catch (err) {
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'register web auth handler failed %{public}s', describeError(err));
    }

    const prefs = this.readPreferencesSafe();
    if (prefs) {
      this.applyPreferences(prefs);
      const params: LaunchParams = {
        preferences: prefs,
        deviceType: deviceInfo.deviceType,
        canScanQr: this.qr.canScanQr(),
        canGenerateQr: this.qr.canGenerateQr(),
        initialDomains: this.readInitialDomainsSafe(prefs),
      };
      surface.setParams(params);
    }

    surface.setOpenUrlHandler((url: string): boolean => {
      if (!/^https?:\/\//i.test(url)) {
        return false;
      }
      void this.context.openLink(url).catch((err: Error): void => {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'openLink failed %{public}s', describeError(err));
      });
      return true;
    });

    surface.handle('core.command', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleCoreCommand(data, reply);
    });
    surface.handle('core.subscribe', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      this.handleCoreSubscribe(data, reply);
    });
    surface.handle('core.query', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleCoreQuery(data, reply);
    });
    surface.handle('platform.pickCertFile', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handlePickCertFile(data, reply);
    });
    surface.handle('platform.exportLog', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleExportLog(data, reply);
    });
    surface.handle('platform.openUrl', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleOpenUrl(data, reply);
    });
    surface.handle('platform.qr', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleGenerateQr(data, reply);
    });
    surface.handle('platform.exportQr', (data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleExportQr(data, reply);
    });
    surface.handle('platform.scanQr', (_data: string | number | boolean | object | null, reply: NshHostReply): void => {
      void this.handleScanQr(reply);
    });
    surface.handle('platform.syncAppearance', (_data: string | number | boolean | object | null, reply: NshHostReply): void => {
      this.refreshAppearance();
      reply.resolve({ ok: true } as SimpleOk);
    });

    void this.observeRevisions(generation);
    void this.observePlatformChanges(generation);
  }

  detachSurface(): void {
    ++this.surfaceGeneration;
    this.stopConnectedTimer();
    this.closeWebAuthSession();
    try {
      hopenconnectUi.clearExternalBrowserHandler();
    } catch (_err) {
      // ignore
    }
    try {
      hopenconnectUi.cancelUiRevisionWait();
      hopenconnectUi.cancelPlatformChangeWait();
    } catch (_err) {
      // ignore
    }
    if (this.authSession) {
      try {
        this.authSession.closeAll();
      } catch (_err) {
        // ignore
      }
      this.authSession = undefined;
    }
    this.surface = undefined;
  }

  refreshAppearance(): void {
    const prefs = this.readPreferencesSafe();
    if (!prefs) {
      return;
    }
    this.applyPreferences(prefs);
    this.publishPreferences(prefs);
  }

  onForeground(): void {
    try {
      hopenconnectUi.syncPlatformChanges();
      this.runSessionTick();
    } catch (_err) {
      // ignore
    }
  }

  dispose(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    this.detachSurface();
    this.mainWindow = undefined;
    this.session.close();
    this.vpn.detachHost();
  }

  private openWebAuthSession(uri: string): boolean {
    const trimmed = uri.trim();
    if (!/^https?:\/\//i.test(trimmed)) {
      return false;
    }
    const auth = this.authSession;
    if (!auth) {
      return false;
    }
    this.syncAuthSessionColorMode();
    const key = this.nextAuthKey++;
    this.activeAuthKey = key;
    this.closingAuthProgrammatically = false;
    try {
      // OpenConnect SSO-v2 completes via its local loopback callback server
      // (`http://[::1]:29786/...`), so we pass a non-HTTP custom callbackScheme
      // allowing all HTTP/HTTPS IdP & loopback redirects inside upstream's
      // NativeAuthSession sheet. When OpenConnect finishes obtain_cookie,
      // `closeWebAuthSession()` dismisses the sheet automatically.
      auth.open(key, trimmed, 'hopenconnect', false, (reply: SystemReply): void => {
        if (this.activeAuthKey === key) {
          this.activeAuthKey = 0;
        }
        if (this.closingAuthProgrammatically) {
          return;
        }
        if (reply.phase === 'cancelled' || (reply.error && reply.error.length > 0)) {
          try {
            hopenconnectUi.cancelAuthChallenge();
          } catch (_err) {
            // ignore
          }
        }
      });
      return true;
    } catch (err) {
      if (this.activeAuthKey === key) {
        this.activeAuthKey = 0;
      }
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'NativeAuthSession.open failed %{public}s', describeError(err));
      return false;
    }
  }

  private closeWebAuthSession(): void {
    const auth = this.authSession;
    const key = this.activeAuthKey;
    if (!auth || key === 0) {
      return;
    }
    this.activeAuthKey = 0;
    this.closingAuthProgrammatically = true;
    try {
      auth.close(key);
    } catch (_err) {
      // ignore
    } finally {
      this.closingAuthProgrammatically = false;
    }
  }

  private syncAuthSessionColorMode(): void {
    if (!this.authSession) {
      return;
    }
    const prefs = this.readPreferencesSafe();
    if (!prefs || prefs.theme === 'system') {
      this.authSession.setColorMode(ThemeColorMode.SYSTEM);
    } else if (prefs.theme === 'dark') {
      this.authSession.setColorMode(ThemeColorMode.DARK);
    } else {
      this.authSession.setColorMode(ThemeColorMode.LIGHT);
    }
  }

  private async handleCoreCommand(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    if (this.closed) {
      reply.reject('Host session is closed');
      return;
    }
    try {
      const request = data as UiRequest;
      if (!request || typeof request.command !== 'string') {
        reply.reject('Invalid command request');
        return;
      }
      const responseJson = await this.session.dispatch(JSON.stringify(request));
      const result = JSON.parse(responseJson) as UiResult;

      if (request.command === 'vpn.cancelConnect' ||
        request.command === 'vpn.disconnect' ||
        request.command === 'vpn.resetState' ||
        request.command === 'auth.cancel') {
        this.closeWebAuthSession();
      }

      let startConnectInBackground = false;
      if (result.ok && result.effect) {
        if (result.effect.kind === 'connect') {
          startConnectInBackground = true;
        } else if (request.command === 'vpn.resetState' && result.effect.kind === 'stop') {
          await this.applyResetEffect();
        } else {
          await this.applyPlatformEffect(result.effect);
        }
      }

      this.publishPreferences(this.readPreferencesSafe());
      const criticalOnly =
        request.command === 'vpn.disconnect' ||
        request.command === 'vpn.cancelConnect' ||
        request.command === 'vpn.resetState';
      const observation = this.publishChanges(criticalOnly);
      if (result.ok && observation) {
        result.barrier = observation.barrier;
      }
      this.updateConnectedTickerFromTick();
      reply.resolve(result);
      if (startConnectInBackground) {
        void this.runConnectBackground();
      }
    } catch (err) {
      this.closeWebAuthSession();
      this.publishChanges(false);
      reply.reject(describeError(err) || 'Command failed');
    }
  }

  private handleCoreSubscribe(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): void {
    if (this.closed) {
      reply.reject('Host session is closed');
      return;
    }
    try {
      const changeJson = this.session.subscribeState(JSON.stringify(data));
      const change = JSON.parse(changeJson) as StateChanged;
      reply.resolve(change);
    } catch (err) {
      reply.reject(describeError(err) || 'Subscribe failed');
    }
  }

  private async handleCoreQuery(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    if (this.closed) {
      reply.reject('Host session is closed');
      return;
    }
    try {
      const responseJson = await this.session.queryState(JSON.stringify(data));
      const parsed = JSON.parse(responseJson) as Object;
      reply.resolve(parsed);
    } catch (err) {
      reply.reject(describeError(err) || 'Query failed');
    }
  }

  private async handlePickCertFile(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    try {
      const args = (data as PickCertArgs | null) ?? {};
      const kind = typeof args.kind === 'string' && args.kind.length > 0 ? args.kind : 'certificate';
      const path = await this.certFiles.pickCertFile(kind);
      const result: PickCertResult = { path };
      reply.resolve(result);
    } catch (err) {
      reply.reject(describeError(err) || 'Certificate selection failed');
    }
  }

  private async handleExportLog(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    try {
      const args = (data as ExportLogArgs | null) ?? {};
      const fileName = typeof args.fileName === 'string' && args.fileName.length > 0
        ? args.fileName
        : 'h-openconnect-log.log';
      let content = typeof args.content === 'string' ? args.content : '';
      if (content.length === 0 && args.fileName && args.fileName.length > 0) {
        const readReq: UiRequest = {
          id: 0,
          command: 'logs.readArchive',
          payload: { fileName: args.fileName } as ExportLogArgs,
        };
        const resp = JSON.parse(await this.session.dispatch(JSON.stringify(readReq))) as UiResult;
        if (!resp.ok || typeof resp.data !== 'string') {
          throw new Error(resp.error || 'Unable to read log archive');
        }
        content = resp.data;
      }
      await this.exporter.exportLog(fileName, content);
      reply.resolve({ ok: true } as SimpleOk);
    } catch (err) {
      reply.reject(describeError(err) || 'Export log failed');
    }
  }

  private async handleOpenUrl(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    try {
      const args = (data as OpenUrlArgs | null) ?? {};
      const url = typeof args.url === 'string' ? args.url.trim() : '';
      if (!/^https?:\/\//i.test(url)) {
        throw new Error('Only HTTP and HTTPS links are supported');
      }
      await this.context.openLink(url);
      reply.resolve({ ok: true } as SimpleOk);
    } catch (err) {
      reply.reject(describeError(err) || 'Open link failed');
    }
  }

  private async handleGenerateQr(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    try {
      const args = (data as QrGenerateArgs | null) ?? {};
      const text = typeof args.text === 'string' ? args.text : '';
      const image = await this.qr.generateQr(text);
      reply.resolve(image);
    } catch (err) {
      reply.reject(describeError(err) || 'Generate QR code failed');
    }
  }

  private async handleExportQr(
    data: string | number | boolean | object | null,
    reply: NshHostReply,
  ): Promise<void> {
    try {
      const args = (data as QrExportArgs | null) ?? {};
      const name = typeof args.name === 'string' && args.name.length > 0 ? args.name : 'connection';
      const pngBase64 = typeof args.pngBase64 === 'string' ? args.pngBase64 : '';
      await this.exporter.exportImage(name, pngBase64);
      reply.resolve({ ok: true } as SimpleOk);
    } catch (err) {
      reply.reject(describeError(err) || 'Export QR image failed');
    }
  }

  private async handleScanQr(reply: NshHostReply): Promise<void> {
    try {
      const content = await this.qr.scanQr(this.context);
      const result: ScanQrResult = { content };
      reply.resolve(result);
    } catch (err) {
      reply.reject(describeError(err) || 'Scan QR code failed');
    }
  }

  private async runConnectBackground(): Promise<void> {
    if (this.closed || this.connectInFlight) {
      return;
    }
    this.connectInFlight = true;
    try {
      const optionsJson = await hopenconnectUi.prepareVpn();
      this.closeWebAuthSession();
      this.publishChanges(false);
      if (this.closed) {
        return;
      }
      // Match origin/main's fire-and-forget tunnel start so neither interactive
      // authentication nor extension startup ever holds a UI host call open.
      void this.vpn
        .requestStartVpn(optionsJson)
        .catch((err: Error): void => {
          hilog.warn(LOG_DOMAIN, LOG_TAG, 'background tunnel start failed %{public}s', describeError(err));
        })
        .finally((): void => {
          this.publishChanges(false);
          this.updateConnectedTickerFromTick();
        });
    } catch (err) {
      this.closeWebAuthSession();
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'prepare connect failed %{public}s', describeError(err));
      this.publishChanges(false);
    } finally {
      this.connectInFlight = false;
      this.publishChanges(false);
      this.updateConnectedTickerFromTick();
    }
  }

  private async applyResetEffect(): Promise<void> {
    try {
      await this.vpn.requestStopVpn(true);
    } catch (err) {
      hilog.warn(LOG_DOMAIN, LOG_TAG, 'best-effort tunnel stop during reset skipped %{public}s', describeError(err));
    } finally {
      try {
        await hopenconnectUi.finalizeDisconnect();
      } catch (err) {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'finalizeDisconnect during reset failed %{public}s', describeError(err));
      }
    }
  }

  private async applyPlatformEffect(effect: UiEffect): Promise<void> {
    if (effect.kind === 'connect') {
      void this.runConnectBackground();
    } else if (effect.kind === 'start') {
      await this.vpn.requestStartVpn(effect.optionsJson);
    } else if (effect.kind === 'stop') {
      try {
        await this.vpn.requestStopVpn();
      } finally {
        await hopenconnectUi.finalizeDisconnect();
      }
    } else {
      throw new Error(`Unsupported platform effect '${effect.kind}'`);
    }
  }

  private readPreferencesSafe(): UiPreferences | undefined {
    try {
      const json = hopenconnectUi.queryUiPreferences();
      return JSON.parse(json) as UiPreferences;
    } catch (_err) {
      return undefined;
    }
  }

  private readInitialDomainsSafe(prefs: UiPreferences): InitialDomains | undefined {
    try {
      const json = hopenconnectUi.querySession();
      const snap = JSON.parse(json) as SessionSnapshotSeed;
      const lifecycle = typeof snap.lifecycle === 'string' && snap.lifecycle.length > 0
        ? snap.lifecycle
        : 'disconnected';
      const busy =
        lifecycle === 'connecting' ||
        lifecycle === 'authenticating' ||
        lifecycle === 'establishing' ||
        lifecycle === 'disconnecting';
      const activeConnectionId = snap.activeConnectionId ?? null;
      const sdkReady = Boolean(snap.sdkReady);
      const backend = typeof snap.backend === 'string' ? snap.backend : '';
      const seeded: InitialDomains = {
        config: {
          activeConnectionId,
          language: prefs.language,
          theme: prefs.theme,
          systemLocale: prefs.systemLocale,
          systemColorMode: prefs.systemColorMode,
          sdkReady,
          backend,
        },
        status: {
          lifecycle,
          vpnSessionId: '',
          lastError: snap.lastError ?? null,
          sdkReady,
          backend,
          pendingAuth: snap.pendingAuth ?? null,
          operationPending: busy,
        },
        profiles: {
          activeConnectionId,
          profiles: Array.isArray(snap.connections) ? snap.connections : [],
        },
        about: {
          appVersion: typeof snap.appVersion === 'string' ? snap.appVersion : '',
          openconnectVersion: snap.anyconnectVersion ?? null,
          backend,
          sdkReady,
        },
      };
      if (snap.stats && snap.network) {
        seeded.statistics = {
          lifecycle,
          stats: snap.stats,
          network: snap.network,
          trafficHistory: [],
        };
      }
      return seeded;
    } catch (_err) {
      return undefined;
    }
  }

  private publishPreferences(prefs?: UiPreferences): void {
    if (!prefs || !this.surface || this.surface.isDisposed) {
      return;
    }
    const serialized = JSON.stringify(prefs);
    if (serialized === this.publishedPreferences) {
      return;
    }
    this.publishedPreferences = serialized;
    this.applyPreferences(prefs);
    this.surface.send('core.preferences', prefs);
  }

  private applyPreferences(preferences: UiPreferences): void {
    const dark =
      preferences.theme === 'dark' ||
      (preferences.theme === 'system' &&
        preferences.systemColorMode === ConfigurationConstant.ColorMode.COLOR_MODE_DARK);
    this.syncAuthSessionColorMode();

    if (deviceInfo.deviceType === '2in1') {
      return;
    }
    const target = this.mainWindow;
    if (!target || this.requestedBarDark === dark) {
      return;
    }
    this.requestedBarDark = dark;
    this.barUpdates = this.barUpdates
      .then(async (): Promise<void> => {
        if (this.closed || this.mainWindow !== target || this.requestedBarDark !== dark) {
          return;
        }
        await target.setWindowBackgroundColor('#00000000');
        if (this.closed || this.mainWindow !== target || this.requestedBarDark !== dark) {
          return;
        }
        const content = dark ? '#ffffffff' : '#ff000000';
        await target.setWindowSystemBarProperties({
          statusBarColor: '#00000000',
          navigationBarColor: '#00000000',
          statusBarContentColor: content,
          navigationBarContentColor: content,
        });
      })
      .catch((error: Error): void => {
        if (this.mainWindow === target && this.requestedBarDark === dark) {
          this.requestedBarDark = undefined;
        }
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'system bar appearance failed %{public}s', describeError(error));
      });
  }

  private emitChanged(change: StateChanged): void {
    if (!this.surface || this.surface.isDisposed) {
      return;
    }
    if (change.changes.length > 0) {
      this.surface.send('core.changed', change);
    }
  }

  private publishChanges(criticalOnly: boolean): StateObservation | undefined {
    if (this.closed) {
      return undefined;
    }
    try {
      const observation = JSON.parse(
        this.session.observeState(criticalOnly),
      ) as StateObservation;
      this.emitChanged(observation.change);
      return observation;
    } catch (_err) {
      return undefined;
    }
  }

  private runSessionTick(): TickOutcome | undefined {
    if (this.closed) {
      return undefined;
    }
    try {
      const outcome = JSON.parse(this.session.tickSession()) as TickOutcome;
      if (outcome.lifecycle === 'connected') {
        this.startConnectedTimer();
        this.closeWebAuthSession();
      } else {
        this.stopConnectedTimer();
        if (outcome.lifecycle === 'establishing' ||
          outcome.lifecycle === 'disconnected' ||
          outcome.lifecycle === 'failed') {
          this.closeWebAuthSession();
        }
      }
      if (outcome.shouldReconnect && !this.autoReconnectInFlight) {
        this.triggerAutoReconnect();
      }
      return outcome;
    } catch (_err) {
      return undefined;
    }
  }

  private updateConnectedTickerFromTick(): void {
    this.runSessionTick();
  }

  private startConnectedTimer(): void {
    if (this.connectedTimer >= 0 || this.closed) {
      return;
    }
    this.connectedTimer = setInterval((): void => {
      if (this.closed || !this.surface || this.surface.isDisposed) {
        this.stopConnectedTimer();
        return;
      }
      this.runSessionTick();
      this.publishChanges(false);
    }, 1000);
  }

  private stopConnectedTimer(): void {
    if (this.connectedTimer >= 0) {
      clearInterval(this.connectedTimer);
      this.connectedTimer = -1;
    }
  }

  private triggerAutoReconnect(): void {
    this.autoReconnectInFlight = true;
    const req: UiRequest = {
      id: ++this.autoReconnectId,
      command: 'vpn.connect',
      payload: new Object(),
    };
    void this.session
      .dispatch(JSON.stringify(req))
      .then(async (responseJson: string): Promise<void> => {
        const result = JSON.parse(responseJson) as UiResult;
        if (result.ok && result.effect) {
          await this.applyPlatformEffect(result.effect);
        }
      })
      .catch((err: Error): void => {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'auto-reconnect failed %{public}s', describeError(err));
      })
      .finally((): void => {
        this.autoReconnectInFlight = false;
        this.publishChanges(false);
      });
  }

  private async observeRevisions(generation: number): Promise<void> {
    let revision = '0';
    const initial = this.publishChanges(false);
    if (initial) {
      revision = String(initial.change.revision);
    }
    this.publishPreferences(this.readPreferencesSafe());
    this.runSessionTick();

    while (!this.closed && generation === this.surfaceGeneration) {
      try {
        const observed = await hopenconnectUi.waitForUiRevision(revision);
        if (this.closed || generation !== this.surfaceGeneration) {
          return;
        }
        this.publishPreferences(this.readPreferencesSafe());
        if (observed !== revision) {
          const obs = this.publishChanges(false);
          revision = obs ? String(obs.change.revision) : observed;
        }
        this.runSessionTick();
      } catch (err) {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'revision observer error %{public}s', describeError(err));
        await new Promise<void>((resolve): void => {
          setTimeout((): void => resolve(), 500);
        });
      }
    }
  }

  private async observePlatformChanges(generation: number): Promise<void> {
    while (!this.closed && generation === this.surfaceGeneration) {
      try {
        const changed = await hopenconnectUi.waitForPlatformChangeEvent();
        if (this.closed || generation !== this.surfaceGeneration) {
          return;
        }
        if (!changed) {
          await new Promise<void>((resolve): void => {
            setTimeout((): void => resolve(), 200);
          });
          continue;
        }
        hopenconnectUi.syncPlatformChanges();
        const pendingBrowserJson = hopenconnectUi.takePendingBrowserOpen();
        if (pendingBrowserJson && pendingBrowserJson.length > 0) {
          const req = JSON.parse(pendingBrowserJson) as BrowserOpenRequest;
          if (req && req.uri) {
            this.openWebAuthSession(req.uri);
          }
        }
        this.runSessionTick();
        this.publishChanges(false);
      } catch (err) {
        hilog.warn(LOG_DOMAIN, LOG_TAG, 'platform change observer error %{public}s', describeError(err));
        await new Promise<void>((resolve): void => {
          setTimeout((): void => resolve(), 500);
        });
      }
    }
  }
}

let currentHost: HOpenHostServices | undefined;

export function installHOpenHost(context: common.UIAbilityContext): HOpenHostServices {
  currentHost?.dispose();
  currentHost = new HOpenHostServices(context);
  return currentHost;
}

export function hopenHost(): HOpenHostServices {
  if (!currentHost) {
    throw new Error('H-OpenConnect host services are not initialized');
  }
  return currentHost;
}
