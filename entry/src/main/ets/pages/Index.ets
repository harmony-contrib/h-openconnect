import { DefaultXComponent } from '@ohos-rs/ability';
import hopenconnectUi from 'libhopenconnect_ui.so';

@Entry
@Component
struct Index {
  onBackPress(): boolean | void {
    if (typeof hopenconnectUi.onBackPressIntercept === 'function') {
      return hopenconnectUi.onBackPressIntercept();
    }
    return false;
  }

  build() {
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
  }
}
