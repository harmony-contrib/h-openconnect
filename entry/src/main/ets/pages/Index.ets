import { DefaultXComponent } from '@ohos-rs/ability';
import hanyconnectUi from 'libhanyconnect_ui.so';

@Entry
@Component
struct Index {
  onBackPress(): boolean | void {
    if (typeof hanyconnectUi.onBackPressIntercept === 'function') {
      return hanyconnectUi.onBackPressIntercept();
    }
    return false;
  }

  build() {
    RelativeContainer() {
      DefaultXComponent({ moduleName: 'hanyconnect_ui' })
        .height('100%')
        .width('100%')
        .alignRules({
          top: { anchor: '__container__', align: VerticalAlign.Top },
          bottom: { anchor: '__container__', align: VerticalAlign.Bottom },
          left: { anchor: '__container__', align: HorizontalAlign.Start },
          right: { anchor: '__container__', align: HorizontalAlign.End },
        })
        .id('hanyconnectRoot')
    }
    .height('100%')
    .width('100%')
  }
}
