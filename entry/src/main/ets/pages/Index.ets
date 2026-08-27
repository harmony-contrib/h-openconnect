import { common, ConfigurationConstant } from '@kit.AbilityKit';
import { i18n } from '@kit.LocalizationKit';
import { DefaultXComponent } from '@ohos-rs/ability';
import hopenconnectUi from 'libhopenconnect_ui.so';
import { configureNativePlatformIdentityIfConsented } from '../common/PlatformIdentity';
import { grantPrivacyConsent, hasPrivacyConsent } from '../common/PrivacyConsent';

@Entry
@Component
struct Index {
  @State privacyAccepted: boolean = false;
  @State useChinese: boolean = true;
  @State darkMode: boolean = false;

  private abilityContext: common.UIAbilityContext =
    this.getUIContext().getHostContext() as common.UIAbilityContext;

  aboutToAppear(): void {
    this.useChinese = i18n.System.getSystemLanguage().toLowerCase().startsWith('zh');
    this.darkMode = this.abilityContext.config.colorMode ===
      ConfigurationConstant.ColorMode.COLOR_MODE_DARK;
    this.privacyAccepted = hasPrivacyConsent(this.abilityContext);
  }

  private togglePrivacyLanguage(): void {
    this.useChinese = !this.useChinese;
  }

  private agreeAndContinue(): void {
    try {
      if (!grantPrivacyConsent(this.abilityContext)) {
        this.showPrivacySaveError();
        return;
      }
      configureNativePlatformIdentityIfConsented(this.abilityContext, hopenconnectUi);
      this.privacyAccepted = true;
    } catch (_err) {
      this.showPrivacySaveError();
    }
  }

  private showPrivacySaveError(): void {
    try {
      this.getUIContext().getPromptAction().showToast({
        message: this.useChinese ? '保存隐私设置失败，请重试' : 'Could not save privacy settings. Try again.',
      });
    } catch (_err) {
    }
  }

  private declineAndExit(): void {
    this.abilityContext.terminateSelf().catch(() => {});
  }

  onBackPress(): boolean | void {
    if (!this.privacyAccepted) {
      this.declineAndExit();
      return true;
    }
    if (typeof hopenconnectUi.onBackPressIntercept === 'function') {
      return hopenconnectUi.onBackPressIntercept();
    }
    return false;
  }

  build() {
    if (this.privacyAccepted) {
      RelativeContainer() {
        DefaultXComponent({ moduleName: 'hopenconnect_ui' })
          .height('100%')
          .width('100%')
          .alignRules({
            top: { anchor: '__container__', align: VerticalAlign.Top },
            bottom: { anchor: '__container__', align: VerticalAlign.Bottom },
            left: { anchor: '__container__', align: HorizontalAlign.Start },
            right: { anchor: '__container__', align: HorizontalAlign.End },
          })
          .id('hopenconnectRoot')
      }
      .height('100%')
      .width('100%')
    } else {
      Column() {
        Row() {
          Text(this.useChinese ? '隐私政策' : 'Privacy Policy')
            .fontSize(20)
            .fontWeight(FontWeight.Bold)
            .fontColor(this.primaryTextColor())
            .layoutWeight(1)
          Button() {
            Text(this.useChinese ? 'EN' : '中')
              .fontSize(13)
              .fontWeight(FontWeight.Medium)
              .fontColor(this.accentColor())
          }
            .type(ButtonType.Normal)
            .width(56)
            .height(32)
            .padding(0)
            .backgroundColor(this.accentMutedColor())
            .borderRadius(8)
            .onClick(() => this.togglePrivacyLanguage())
        }
        .height(56)
        .padding({ left: 16, right: 16 })
        .alignItems(VerticalAlign.Center)
        .backgroundColor(this.surfaceColor())
        .width('100%')

        Divider()
          .strokeWidth(1)
          .color(this.lineColor())

        Scroll() {
          Column() {
            Row() {
              Column() {
                Text('H-OpenConnect')
                  .fontSize(12)
                  .fontWeight(FontWeight.Medium)
                  .fontColor(this.accentColor())
                  .letterSpacing(0.4)
                Text(this.useChinese ? '使用前请确认' : 'Review before use')
                  .fontSize(22)
                  .fontWeight(FontWeight.Bold)
                  .fontColor(this.primaryTextColor())
                  .margin({ top: 5 })
                Text(this.useChinese ?
                  '请阅读以下数据处理说明。只有同意后，应用才会读取设备标识并进入主界面。' :
                  'Review how data is handled. The app reads a device identifier and opens the main interface only after you agree.')
                  .fontSize(14)
                  .fontColor(this.secondaryTextColor())
                  .lineHeight(21)
                  .margin({ top: 8 })
                  .width('100%')
              }
              .layoutWeight(1)
              .alignItems(HorizontalAlign.Start)

              Column() {
                Text('✓')
                  .fontSize(22)
                  .fontWeight(FontWeight.Bold)
                  .fontColor(this.accentColor())
              }
              .width(48)
              .height(48)
              .margin({ left: 14 })
              .alignItems(HorizontalAlign.Center)
              .justifyContent(FlexAlign.Center)
              .backgroundColor(this.accentMutedColor())
              .borderRadius(24)
            }
            .width('100%')
            .alignItems(VerticalAlign.Center)
            .padding({ left: 18, right: 18, top: 18, bottom: 18 })
            .backgroundColor(this.surfaceColor())
            .borderRadius(16)
            .border({ width: 1, color: this.lineColor() })

            Column() {
              if (this.useChinese) {
                this.PolicyRow(
                  '01',
                  '我们收集的信息',
                  '经你同意后，应用读取操作系统版本、设备类型、应用版本和 ODID。应用不收集 OAID，不接入广告、分析或遥测 SDK。',
                  true,
                )
                this.PolicyRow(
                  '02',
                  '使用目的与传输范围',
                  '上述信息用于生成 AnyConnect 客户端身份。仅当你主动连接时，ODID 才会作为终端标识提供给你配置的 VPN 网关；不会上传到 H-OpenConnect 自有服务器。',
                  true,
                )
                this.PolicyRow(
                  '03',
                  '本地数据',
                  '连接配置和凭据保存在应用私有目录，并排除在系统备份之外。诊断日志默认关闭，仅在你主动开启后写入本地。',
                  true,
                )
                this.PolicyRow(
                  '04',
                  '你的选择',
                  '你可以拒绝并退出应用。接受后，可在“更多 > 关于”的隐私章节随时查看数据处理说明；清除应用数据会撤回已保存的同意。',
                  true,
                )
                this.PolicyRow(
                  '05',
                  '联系我们',
                  '如有隐私问题，请通过 github.com/harmony-contrib/h-openconnect 联系项目维护者。政策版本：2026-08-27。',
                  false,
                )
              } else {
                this.PolicyRow(
                  '01',
                  'Information we process',
                  'After you consent, the app reads the operating system version, device type, app version, and ODID. It does not collect OAID or include advertising, analytics, or telemetry SDKs.',
                  true,
                )
                this.PolicyRow(
                  '02',
                  'Purpose and transfer scope',
                  'This information forms the AnyConnect client identity. Only when you connect is ODID provided as a terminal identifier to the VPN gateway you configured. It is not uploaded to an H-OpenConnect server.',
                  true,
                )
                this.PolicyRow(
                  '03',
                  'Local data',
                  'Connection profiles and credentials stay in the app-private directory and are excluded from system backup. Diagnostic logging is off by default and writes locally only after you enable it.',
                  true,
                )
                this.PolicyRow(
                  '04',
                  'Your choice',
                  'You may decline and exit. After accepting, you can review data handling under More > About. Clearing the app data withdraws the saved consent.',
                  true,
                )
                this.PolicyRow(
                  '05',
                  'Contact',
                  'For privacy questions, contact the maintainers at github.com/harmony-contrib/h-openconnect. Policy version: 2026-08-27.',
                  false,
                )
              }
            }
            .width('100%')
            .margin({ top: 14, bottom: 18 })
            .padding({ left: 16, right: 16 })
            .backgroundColor(this.surfaceColor())
            .borderRadius(16)
            .border({ width: 1, color: this.lineColor() })
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 16 })
          .alignItems(HorizontalAlign.Start)
        }
        .layoutWeight(1)
        .scrollBar(BarState.Auto)
        .width('100%')
        .backgroundColor(this.privacyBackgroundColor())

        Row({ space: 12 }) {
          Button() {
            Text(this.useChinese ? '不同意并退出' : 'Decline and exit')
              .fontSize(15)
              .fontWeight(FontWeight.Medium)
              .fontColor(this.primaryTextColor())
          }
            .type(ButtonType.Normal)
            .padding(0)
            .backgroundColor(this.surfaceColor())
            .border({ width: 1, color: this.lineColor() })
            .borderRadius(10)
            .layoutWeight(1)
            .height(48)
            .onClick(() => this.declineAndExit())
          Button() {
            Text(this.useChinese ? '同意并继续' : 'Agree and continue')
              .fontSize(15)
              .fontWeight(FontWeight.Bold)
              .fontColor(Color.White)
          }
            .type(ButtonType.Normal)
            .padding(0)
            .backgroundColor(this.accentColor())
            .borderRadius(10)
            .layoutWeight(1)
            .height(48)
            .onClick(() => this.agreeAndContinue())
        }
        .width('100%')
        .padding({ left: 16, right: 16, top: 14, bottom: 14 })
        .backgroundColor(this.surfaceColor())
        .border({ width: { top: 1 }, color: this.lineColor() })
      }
      .height('100%')
      .width('100%')
      .backgroundColor(this.privacyBackgroundColor())
    }
  }

  @Builder
  PolicyRow(index: string, title: string, content: string, showDivider: boolean) {
    Column() {
      Row() {
        Column() {
          Text(index)
            .fontSize(11)
            .fontWeight(FontWeight.Bold)
            .fontColor(this.accentColor())
        }
        .width(34)
        .height(34)
        .margin({ right: 12 })
        .alignItems(HorizontalAlign.Center)
        .justifyContent(FlexAlign.Center)
        .backgroundColor(this.accentMutedColor())
        .borderRadius(9)

        Column() {
          Text(title)
            .fontSize(15)
            .fontWeight(FontWeight.Medium)
            .fontColor(this.primaryTextColor())
            .width('100%')
          Text(content)
            .fontSize(13)
            .fontColor(this.secondaryTextColor())
            .lineHeight(20)
            .margin({ top: 5 })
            .width('100%')
        }
        .layoutWeight(1)
        .alignItems(HorizontalAlign.Start)
      }
      .width('100%')
      .alignItems(VerticalAlign.Top)
      .padding({ top: 16, bottom: 16 })

      if (showDivider) {
        Divider()
          .strokeWidth(1)
          .color(this.lineColor())
          .margin({ left: 46 })
      }
    }
    .width('100%')
    .alignItems(HorizontalAlign.Start)
  }

  private privacyBackgroundColor(): string {
    return this.darkMode ? '#0B1017' : '#F8FAFC';
  }

  private surfaceColor(): string {
    return this.darkMode ? '#151B24' : '#FFFFFF';
  }

  private primaryTextColor(): string {
    return this.darkMode ? '#F8FAFC' : '#101828';
  }

  private secondaryTextColor(): string {
    return this.darkMode ? '#A8B2C1' : '#667085';
  }

  private lineColor(): string {
    return this.darkMode ? '#2A3441' : '#E4E7EC';
  }

  private accentColor(): string {
    return this.darkMode ? '#60A5FA' : '#1D4ED8';
  }

  private accentMutedColor(): string {
    return this.darkMode ? '#172A46' : '#EEF4FF';
  }
}
