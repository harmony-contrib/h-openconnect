import {
  AbilityConstant,
  Configuration,
  ConfigurationConstant,
  UIAbility,
  Want,
} from '@kit.AbilityKit';
import { deviceInfo } from '@kit.BasicServicesKit';
import { i18n } from '@kit.LocalizationKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { initLogging } from '@nsh/harmonyos';
import window from '@ohos.window';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { describeError } from '../common/Errors';
import { configureNativePlatformIdentity } from '../common/PlatformIdentity';
import { HOpenHostServices, installHOpenHost } from '../host/HOpenHostServices';
import { setHOpenBundleName } from '../vpnability/VpnConfig';

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = 'HOpenConnectEntry';

/**
 * Thin HarmonyOS Ability entry point: configures native storage/identity,
 * installs `HOpenHostServices`, and loads the NSH surface page.
 */
export default class EntryAbility extends UIAbility {
  private hostServices?: HOpenHostServices;
  private lifecycleQueue: Promise<void> = Promise.resolve();

  private enqueueLifecycle(operation: () => Promise<void>): Promise<void> {
    const next = this.lifecycleQueue.then(operation, operation);
    this.lifecycleQueue = next.catch((): void => {});
    return next;
  }

  private configureNativeHome(): void {
    setHOpenBundleName(this.context.applicationInfo.name);
    hopenconnectUi.configureAppHome(`${this.context.filesDir}/h-openconnect`);
    hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native home');
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
      hopenconnectUi.configureUiLocale(locale);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native locale %{public}s', locale);
    } catch (err) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configure native locale skipped %{public}s', describeError(err));
    }
  }

  private configureNativeAppearance(colorMode?: ConfigurationConstant.ColorMode): void {
    try {
      const mode =
        colorMode ??
        this.context.config.colorMode ??
        ConfigurationConstant.ColorMode.COLOR_MODE_NOT_SET;
      hopenconnectUi.configureSystemColorMode(mode);
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configured native color mode %{public}d', mode);
    } catch (err) {
      hilog.info(LOG_DOMAIN, LOG_TAG, 'configure native color mode skipped %{public}s', describeError(err));
    }
  }

  public onCreate(_want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    try {
      this.configureNativeHome();
      configureNativePlatformIdentity(this.context, hopenconnectUi);
      this.configureNativeLocale();
      this.configureNativeAppearance();
      this.hostServices = installHOpenHost(this.context);
    } catch (err) {
      const message = describeError(err) || 'native initialization failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'native initialization failed %{public}s', message);
      throw new Error(message);
    }
  }

  public onForeground(): void {
    this.hostServices?.onForeground();
  }

  public onNewWant(_want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    this.hostServices?.onForeground();
  }

  public onWindowStageCreate(windowStage: window.WindowStage): void {
    void this.enqueueLifecycle((): Promise<void> => this.loadWindowStageContent(windowStage));
  }

  public onDestroy(): void {
    this.hostServices?.dispose();
    this.hostServices = undefined;
  }

  public onConfigurationUpdate(newConfig: Configuration): void {
    this.configureNativeLocale(newConfig.language);
    this.configureNativeAppearance(newConfig.colorMode);
    this.hostServices?.refreshAppearance();
  }

  protected async loadWindowStageContent(windowStage: window.WindowStage): Promise<void> {
    initLogging();
    const win = await windowStage.getMainWindow();
    this.hostServices?.attachMainWindow(win);
    if (deviceInfo.deviceType !== '2in1') {
      try {
        await win.setWindowLayoutFullScreen(true);
      } catch (err) {
        hilog.info(LOG_DOMAIN, LOG_TAG, 'full-screen layout skipped %{public}s', describeError(err));
      }
    }
    try {
      await win.setUIContent('pages/Index');
    } catch (err) {
      const message = describeError(err) || 'set UI content failed';
      hilog.error(LOG_DOMAIN, LOG_TAG, 'set UI content failed %{public}s', message);
      throw new Error(message);
    }
  }
}
