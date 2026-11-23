import { NshSurface } from '@nsh/harmonyos';
import { hilog } from '@kit.PerformanceAnalysisKit';
import BuildProfile from 'BuildProfile';
import { describeError } from '../common/Errors';
import { hopenHost } from '../host/HOpenHostServices';

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface(this.getUIContext());
  private contentMounted: boolean = false;
  @State loadError: string = '';
  @State loadFailed: boolean = false;

  aboutToAppear(): void {
    const host = hopenHost();
    host.attachSurface(this.surface, this.getUIContext());
    this.surface.setErrorHandler((message: string): void => {
      this.reportError(message, !this.contentMounted || !this.surface.isRunning);
    });
    void this.loadBundle();
  }

  private reportError(message: string, loadFailed: boolean): void {
    hilog.error(0x0000, 'HOpenNsh', 'surface failed (startup=%{public}s) %{public}s', String(loadFailed), message);
    if (this.surface.isDisposed) {
      return;
    }
    if (BuildProfile.DEBUG) {
      this.loadError = message;
    }
    if (loadFailed) {
      this.loadFailed = true;
    }
  }

  private async loadBundle(): Promise<void> {
    if (this.surface.isDisposed) {
      return;
    }
    this.loadError = '';
    this.loadFailed = false;
    this.contentMounted = false;
    try {
      await this.surface.loadResource('hopenconnect.bundle.js');
      this.contentMounted = true;
      hilog.info(0x0000, 'HOpenNsh', 'H-OpenConnect bundle loaded');
    } catch (error) {
      this.reportError(describeError(error), true);
    }
  }

  aboutToDisappear(): void {
    hopenHost().detachSurface();
    this.surface.dispose();
  }

  onBackPress(): boolean {
    return this.surface.onBackPress();
  }

  build() {
    Stack() {
      ContentSlot(this.surface.content)
      if (this.loadFailed) {
        Column({ space: 16 }) {
          Text($r('app.string.surface_load_failed'))
            .fontColor($r('sys.color.ohos_id_color_text_primary'));
          Button($r('app.string.surface_retry'))
            .onClick((): void => {
              void this.loadBundle();
            });
        }
        .padding(24)
        .width('100%')
        .height('100%')
        .justifyContent(FlexAlign.Center)
        .backgroundColor($r('sys.color.ohos_id_color_background'));
      }
      if (BuildProfile.DEBUG && this.loadError.length > 0) {
        Text(this.loadError)
          .padding(20)
          .fontColor('#b42318')
          .backgroundColor('#fff4ed')
          .id('hopenconnect-load-error');
      }
    }
    .height('100%')
    .width('100%')
    .expandSafeArea([SafeAreaType.SYSTEM], [SafeAreaEdge.TOP, SafeAreaEdge.BOTTOM])
    .backgroundColor(Color.Transparent)
  }
}
