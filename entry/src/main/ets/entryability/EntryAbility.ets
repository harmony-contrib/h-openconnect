import { AbilityConstant, common, Configuration, ConfigurationConstant, Want } from '@kit.AbilityKit';
import { i18n } from '@kit.LocalizationKit';
import { vpnExtension } from '@kit.NetworkKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { picker, fileIo as fs } from '@kit.CoreFileKit';
import { NativeAbility } from '@ohos-rs/ability';
import hanyconnectUi from 'libhanyconnect_ui.so';
import window from '@ohos.window';
import { buildVpnWant, setHAnyBundleName } from '../vpnability/VpnConfig';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HAnyConnectEntry';

interface HAnyE2eParams {
  hanyServer?: string;
  hanyName?: string;
  hanyGroup?: string;
  hanyUsername?: string;
  hanyPassword?: string;
  hanyAcceptUntrusted?: string | boolean;
  hanyAutoConnect?: string | boolean;
  hanyDryRun?: string | boolean;
  hanyExpectConnected?: string | boolean;
  hanyExpectFailure?: string | boolean;
}

interface HAnyE2eConfigPayload {
  server?: string;
  name?: string;
  group?: string;
  username?: string;
  password?: string;
  acceptUntrusted: boolean;
  autoConnect: boolean;
  dryRun: boolean;
  expectConnected: boolean;
  expectFailure: boolean;
}

interface HAnyPickCertRequest {
  id?: number;
  kind?: string;
}

function describeError(err: Error | Object | string | number | boolean | null | undefined): string {
  if (err instanceof Error) {
    return `${err.name}: ${err.message}${err.stack ? ` ${err.stack}` : ''}`;
  }
  if (typeof err === 'string') {
    return err;
  }
  if (typeof err === 'number' || typeof err === 'boolean') {
    return err.toString();
  }
  if (!err) {
    return '';
  }
  const json = JSON.stringify(err);
  return json && json !== '{}' ? json : err.toString();
}

function asBool(value: string | boolean | undefined): boolean {
  return value === true || value === 'true' || value === '1' || value === 'yes';
}

export default class EntryAbility extends NativeAbility {
  public moduleName: string | string[] = 'hanyconnect_ui';
  public defaultPage: boolean = false;
  private vpnStartRequest: Promise<void> | undefined = undefined;

  private configureNativeHome(): void {
    try {
      setHAnyBundleName(this.context.applicationInfo.name);
      hanyconnectUi.configureAppHome(`${this.context.filesDir}/h-anyconnect`);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native home');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'configure native home failed %{public}s', describeError(err));
    }
  }

  private configureNativeLocale(language?: string): void {
    try {
      let locale = language ?? '';
      if (locale.length === 0) {
        locale = i18n.System.getSystemLanguage();
      }
      if (locale.length === 0) {
        return;
      }
      (hanyconnectUi as ESObject).configureUiLocale(locale);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native locale %{public}s', locale);
    } catch (err) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configure native locale skipped %{public}s', describeError(err));
    }
  }

  private configureNativeAppearance(colorMode?: ConfigurationConstant.ColorMode): void {
    try {
      const mode = colorMode ?? this.context.config.colorMode ?? ConfigurationConstant.ColorMode.COLOR_MODE_NOT_SET;
      (hanyconnectUi as ESObject).configureSystemColorMode(mode);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native color mode %{public}d', mode);
    } catch (err) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configure native color mode skipped %{public}s', describeError(err));
    }
  }

  private setAppColorMode(colorMode: number): void {
    let mode = ConfigurationConstant.ColorMode.COLOR_MODE_NOT_SET;
    if (colorMode === ConfigurationConstant.ColorMode.COLOR_MODE_DARK) {
      mode = ConfigurationConstant.ColorMode.COLOR_MODE_DARK;
    } else if (colorMode === ConfigurationConstant.ColorMode.COLOR_MODE_LIGHT) {
      mode = ConfigurationConstant.ColorMode.COLOR_MODE_LIGHT;
    }
    try {
      this.context.setColorMode(mode);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'applied app color mode %{public}d', mode);
    } catch (err) {
      const message = describeError(err);
      hilog.error(LOG_DOMAIN, LOG_TAG, 'apply app color mode failed %{public}s', message);
      throw new Error(message);
    }
  }

  private async requestStartVpn(optionsJson: string): Promise<void> {
    if (this.vpnStartRequest) {
      return this.vpnStartRequest;
    }
    const request = this.performStartVpn(optionsJson);
    this.vpnStartRequest = request;
    try {
      await request;
    } finally {
      if (this.vpnStartRequest === request) {
        this.vpnStartRequest = undefined;
      }
    }
  }

  private async performStartVpn(optionsJson: string): Promise<void> {
    try {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'request VPN start received');
      hanyconnectUi.setPlatformVpnStarting(true);
      // The first standards-compliant VpnExtension start is intentionally
      // gated by the system consent dialog. Await the platform result instead
      // of imposing an application timeout while the user is deciding.
      await vpnExtension.startVpnExtensionAbility(buildVpnWant(optionsJson));
      hilog.info(LOG_DOMAIN, LOG_TAG, 'requested VPN start');
    } catch (err) {
      const message = describeError(err) || 'request VPN start failed';
      hanyconnectUi.setPlatformVpnFailed(message);
      hilog.error(LOG_DOMAIN, LOG_TAG, 'request VPN start failed %{public}s', message);
      throw new Error(message);
    }
  }

  private async requestStopVpn(): Promise<void> {
    try {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'request VPN stop received');
      await vpnExtension.stopVpnExtensionAbility(buildVpnWant());
      hanyconnectUi.setPlatformVpnRunning(false);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'requested VPN stop');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'request VPN stop failed %{public}s', describeError(err));
      throw new Error(describeError(err) || 'request VPN stop failed');
    }
  }

  /**
   * OpenConnect SSO-v2 external browser: launch system browser with IdP URL.
   * OpenConnect listens on http://[::1]:29786/api/sso/… for the return hop.
   */
  private async openExternalBrowser(uri: string): Promise<void> {
    const target = (uri ?? '').trim();
    if (target.length === 0) {
      throw new Error('empty external browser uri');
    }
    try {
      const want: Want = {
        action: 'ohos.want.action.viewData',
        entities: ['entity.system.browsable'],
        uri: target,
      };
      await this.context.startAbility(want);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'opened external browser uri_len=%{public}d', target.length);
    } catch (err) {
      const message = describeError(err) || 'open external browser failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'open external browser failed %{public}s', message);
      throw new Error(message);
    }
  }

  /**
   * System document picker for client/CA cert files. Copies into app sandbox so
   * OpenConnect (native) can open a real filesystem path.
   */
  private async pickCertFile(requestJson: string): Promise<void> {
    let requestId = 0;
    try {
      const req = JSON.parse(requestJson) as HAnyPickCertRequest;
      requestId = typeof req.id === 'number' ? req.id : 0;
      const kind = typeof req.kind === 'string' ? req.kind : 'certificate';
      const path = await this.pickAndStoreCertFile(kind);
      hanyconnectUi.completeFilePick(requestId, path);
      hilog.info(
        LOG_DOMAIN,
        LOG_TAG,
        'pick cert file done id=%{public}d kind=%{public}s hasPath=%{public}s',
        requestId,
        kind,
        path && path.length > 0 ? 'yes' : 'no',
      );
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'pick cert file failed %{public}s', describeError(err));
      try {
        hanyconnectUi.completeFilePick(requestId, '');
      } catch (_completeErr) {
        // ignore secondary failure
      }
    }
  }

  private certDir(): string {
    return `${this.context.filesDir}/h-anyconnect/certs`;
  }

  private ensureCertDir(): void {
    const dir = this.certDir();
    try {
      if (!fs.accessSync(dir)) {
        fs.mkdirSync(dir, true);
      }
    } catch (_err) {
      try {
        fs.mkdirSync(dir, true);
      } catch (mkdirErr) {
        hilog.error(LOG_DOMAIN, LOG_TAG, 'mkdir certs failed %{public}s', describeError(mkdirErr));
      }
    }
  }

  private async pickAndStoreCertFile(kind: string): Promise<string> {
    this.ensureCertDir();
    const options = new picker.DocumentSelectOptions();
    options.maxSelectNumber = 1;
    // PEM / DER / PKCS#12 common suffixes (generic, not vendor-specific).
    options.fileSuffixFilters = ['.pem', '.crt', '.cer', '.key', '.p12', '.pfx', '.der', '.ca'];
    const documentPicker = new picker.DocumentViewPicker(this.context as common.UIAbilityContext);
    const uris: Array<string> = await documentPicker.select(options);
    if (!uris || uris.length === 0) {
      return '';
    }
    const srcUri = uris[0];
    const baseName = this.fileNameFromUri(srcUri, kind);
    const destPath = `${this.certDir()}/${Date.now()}_${baseName}`;
    await this.copyUriToPath(srcUri, destPath);
    return destPath;
  }

  private fileNameFromUri(uri: string, kind: string): string {
    try {
      const noQuery = uri.split('?')[0];
      const segments = noQuery.split('/');
      let last = segments.length > 0 ? segments[segments.length - 1] : '';
      last = decodeURIComponent(last);
      if (last && last.indexOf('.') > 0 && last.length < 128) {
        // Sanitize path separators that should not appear in a leaf name.
        return last.replace(/[\\/]/g, '_');
      }
    } catch (_err) {
      // fall through
    }
    const ext = kind === 'private_key' ? 'key' : kind === 'ca_certificate' ? 'crt' : 'pem';
    return `${kind}.${ext}`;
  }

  private async copyUriToPath(srcUri: string, destPath: string): Promise<void> {
    let srcFile: fs.File | undefined = undefined;
    let destFile: fs.File | undefined = undefined;
    try {
      srcFile = fs.openSync(srcUri, fs.OpenMode.READ_ONLY);
      destFile = fs.openSync(
        destPath,
        fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC,
      );
      // Stream copy — certs are small; cap total size to avoid abuse.
      const chunk = new ArrayBuffer(64 * 1024);
      let total = 0;
      const maxBytes = 8 * 1024 * 1024;
      while (true) {
        const n: number = fs.readSync(srcFile.fd, chunk);
        if (n <= 0) {
          break;
        }
        total += n;
        if (total > maxBytes) {
          throw new Error(`certificate file too large (>${maxBytes} bytes)`);
        }
        // writeSync accepts ArrayBuffer; when partial read, write a sliced view.
        if (n === chunk.byteLength) {
          fs.writeSync(destFile.fd, chunk);
        } else {
          fs.writeSync(destFile.fd, chunk.slice(0, n));
        }
      }
      if (total === 0) {
        throw new Error('selected certificate file is empty');
      }
    } finally {
      if (srcFile) {
        try {
          fs.closeSync(srcFile);
        } catch (_e) {
          // ignore
        }
      }
      if (destFile) {
        try {
          fs.closeSync(destFile);
        } catch (_e) {
          // ignore
        }
      }
    }
  }

  private e2eParams(want: Want): HAnyE2eParams {
    return (want.parameters as HAnyE2eParams | undefined) ?? {};
  }

  private async runE2eAutomation(want: Want): Promise<void> {
    const params = this.e2eParams(want);
    const server = typeof params.hanyServer === 'string' ? params.hanyServer : '';
    const autoConnect = asBool(params.hanyAutoConnect);
    if (server.length === 0 && !autoConnect) {
      return;
    }
    const dryRun = params.hanyDryRun === undefined ? true : asBool(params.hanyDryRun);
    const config: HAnyE2eConfigPayload = {
      server: server.length > 0 ? server : undefined,
      name: typeof params.hanyName === 'string' ? params.hanyName : 'E2E Connection',
      group: typeof params.hanyGroup === 'string' ? params.hanyGroup : '',
      username: typeof params.hanyUsername === 'string' ? params.hanyUsername : '',
      password: typeof params.hanyPassword === 'string' ? params.hanyPassword : '',
      acceptUntrusted: asBool(params.hanyAcceptUntrusted),
      autoConnect: autoConnect,
      dryRun: dryRun,
      expectConnected: asBool(params.hanyExpectConnected),
      expectFailure: asBool(params.hanyExpectFailure),
    };
    try {
      // NAPI renames e2e → e2E (applyE2EConfig / e2EConnectActive).
      const applied: string = hanyconnectUi.applyE2EConfig(JSON.stringify(config));
      hilog.info(LOG_DOMAIN, LOG_TAG, 'e2e config applied %{public}s', applied);
      if (autoConnect) {
        const snapshot: string = await hanyconnectUi.e2EConnectActive();
        hilog.info(LOG_DOMAIN, LOG_TAG, 'e2e connect result %{public}s', snapshot);
        if (config.expectFailure) {
          hilog.info(LOG_DOMAIN, LOG_TAG, 'e2e expect failure mode');
        }
      }
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'e2e automation failed %{public}s', describeError(err));
      if (!asBool(params.hanyExpectFailure)) {
        throw new Error(describeError(err) || 'e2e automation failed');
      }
    }
  }

  public async onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): Promise<void> {
    try {
      this.configureNativeHome();
      this.configureNativeLocale();
      this.configureNativeAppearance();
      hanyconnectUi.registerPlatformCallbacks({
        setColorMode: async (colorMode: number) => this.setAppColorMode(colorMode),
        requestStartVpn: async (optionsJson: string) => await this.requestStartVpn(optionsJson),
        requestStopVpn: async () => await this.requestStopVpn(),
        openExternalBrowser: async (uri: string) => await this.openExternalBrowser(uri),
        pickCertFile: async (requestJson: string) => await this.pickCertFile(requestJson),
      });
      hilog.info(LOG_DOMAIN, LOG_TAG, 'registered native platform callbacks');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'register callbacks failed %{public}s', describeError(err));
    }
    await super.onCreate(want, launchParam);
    this.runE2eAutomation(want).catch((err: Error) => {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'create Want automation failed %{public}s', err.message);
    });
  }

  public onNewWant(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    super.onNewWant(want, launchParam);
    this.runE2eAutomation(want).catch((err: Error) => {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'new Want automation failed %{public}s', err.message);
    });
  }

  public onConfigurationUpdate(newConfig: Configuration): void {
    this.configureNativeLocale(newConfig.language);
    this.configureNativeAppearance(newConfig.colorMode);
    super.onConfigurationUpdate(newConfig);
  }

  public async onWindowStageCreate(windowStage: window.WindowStage): Promise<void> {
    await super.onWindowStageCreate(windowStage);
    try {
      const win = await windowStage.getMainWindow();
      await win.setUIContent('pages/Index');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'set UI content failed %{public}s', JSON.stringify(err) ?? '');
    }
  }
}
