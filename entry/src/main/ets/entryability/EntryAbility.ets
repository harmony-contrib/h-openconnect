import { AbilityConstant, Configuration, ConfigurationConstant, Want } from '@kit.AbilityKit';
import { i18n } from '@kit.LocalizationKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { LazyPlugin, NativeAbility } from '@ohos-rs/ability';
import { UrlPlugin } from '@ohos-rs/ability-plugin-url';
import window from '@ohos.window';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { describeError } from '../common/Errors';
import { configureNativePlatformIdentity } from '../common/PlatformIdentity';
import { setHOpenBundleName } from '../vpnability/VpnConfig';
import { CertFilePlugin } from '../plugins/CertFilePlugin';
import { ColorModePlugin } from '../plugins/ColorModePlugin';
import { ExportPlugin } from '../plugins/ExportPlugin';
import { VpnPlugin } from '../plugins/VpnPlugin';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectEntry';

/**
 * Thin assembly point: loads the native module, installs the built-in
 * capability plugin (`ohos.url`) plus the application-owned `hopenconnect.*`
 * plugins, and forwards lifecycle events. All platform logic lives in the
 * plugins.
 */
export default class EntryAbility extends NativeAbility {
  public moduleName: string = 'hopenconnect_ui';
  public defaultPage: boolean = false;

  private vpnPlugin?: VpnPlugin;

  public bridgePlugins = [
    new LazyPlugin(() => new UrlPlugin()),
    // The active session instance is retained for debug automation while the
    // factory still creates a fresh plugin for every Ability session.
    new LazyPlugin(() => this.createVpnPlugin()),
    new LazyPlugin(() => new ColorModePlugin()),
    new LazyPlugin(() => new ExportPlugin()),
    new LazyPlugin(() => new CertFilePlugin()),
  ];

  private createVpnPlugin(): VpnPlugin {
    const plugin = new VpnPlugin();
    this.vpnPlugin = plugin;
    return plugin;
  }

  private requestStartVpn(optionsJson: string): Promise<void> {
    const plugin = this.vpnPlugin;
    if (plugin === undefined) {
      return Promise.reject(new Error('VPN plugin is not attached to the active Ability session'));
    }
    return plugin.requestStartVpn(optionsJson);
  }

  private configureNativeHome(): void {
    try {
      setHOpenBundleName(this.context.applicationInfo.name);
      hopenconnectUi.configureAppHome(`${this.context.filesDir}/h-openconnect`);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native home');
    } catch (err) {
      const message = describeError(err) || 'configure native home failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'configure native home failed %{private}s', message);
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
      (hopenconnectUi as ESObject).configureUiLocale(locale);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native locale %{public}s', locale);
    } catch (err) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configure native locale skipped %{public}s', describeError(err));
    }
  }

  private configureNativeAppearance(colorMode?: ConfigurationConstant.ColorMode): void {
    try {
      const mode = colorMode ?? this.context.config.colorMode ?? ConfigurationConstant.ColorMode.COLOR_MODE_NOT_SET;
      (hopenconnectUi as ESObject).configureSystemColorMode(mode);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native color mode %{public}d', mode);
    } catch (err) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configure native color mode skipped %{public}s', describeError(err));
    }
  }

  public async onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): Promise<void> {
    try {
      this.configureNativeHome();
      configureNativePlatformIdentity(hopenconnectUi);
      this.configureNativeLocale();
      this.configureNativeAppearance();
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'native initialization failed %{public}s', describeError(err));
    }
    await super.onCreate(want, launchParam);
  }

  public onNewWant(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    super.onNewWant(want, launchParam);
  }

  public onConfigurationUpdate(newConfig: Configuration): void {
    this.configureNativeLocale(newConfig.language);
    this.configureNativeAppearance(newConfig.colorMode);
    super.onConfigurationUpdate(newConfig);
  }

  protected async loadWindowStageContent(windowStage: window.WindowStage): Promise<void> {
    try {
      const win = await windowStage.getMainWindow();
      await win.setUIContent('pages/Index');
    } catch (err) {
      hilog.error(LOG_DOMAIN, LOG_TAG, 'set UI content failed %{public}s', JSON.stringify(err) ?? '');
    }
  }
}
