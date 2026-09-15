# Arkit / shadcn 迁移

参考本地 Paws `e5dd4828e43e87d885e6c83f91cfaaecdf34d136`。
2026-09-14 通过 `git ls-remote` 核对，Arkit 上游 HEAD / main 为
`1d4163f2168a49aba1a6fd8ea6ca81406212c4e9`，本项目及锁文件中的
14 个 Arkit crate 均固定到该提交。Dioxus / NAPI 最低版本同步为
0.7.10 / 1.2.0，移除上游已废弃的 arkit_dom 锁文件项。

## 实现

- 公共按钮、分段选择和详情／确认弹窗分别委托给上游
  Button、TabsList / TabsTrigger、Dialog。应用只保留动作语义映射。
- 连接编辑器用 FieldGroup / Field / FieldLabel 替换已移除的
  Form / FormItem；草稿、验证、保存和认证字段映射仍由应用负责。
- 认证界面使用 BottomSheet，按窗口、安全区及键盘遮挡计算内容高度，
  避免自适应弹层内的百分比高度形成循环布局依赖。
- 弹层注册独立的返回键处理，优先关闭弹层；认证弹层关闭会取消当前认证。
- 页面统一采用 shadcn 字号和圆角 token；主操作按 primary /
  primary_foreground 成对取色，修复深色主题中硬编码白色文字的问题。
  成功／警告保留语义颜色。设置菜单的拼接边缘保留直角，
  原生整行按钮显式采用 normal 类型；虚拟日志行显式传递调色板。
- 两个日志列表迁移到 use_virtual_items / VirtualItemStamp。
  归档以文件名标识身份；当前日志以内容及重复出现次数标识身份。
  内容、忙碌状态和调色板哈希只用于显示版本，避免重复日志身份碰撞。
- 关于页的 Arkit 短提交号从工作区 Cargo.toml 在构建时生成。
  现有 URL 插件已经在 Rust 入口注册，不存在 Paws 的漏注册问题。

## 验证

- `cargo check -p hopenconnect_ui --lib --locked --offline` 通过。
- `cargo test --workspace --locked --offline`：100 个测试通过，
  包含重复日志身份与插入稳定性测试；未启用真实服务连接测试。
- `cargo fmt --all --check` 和 `git diff --check` 通过。
- `scripts/package-hap.sh` 完成包含 native-anyconnect 的 release HAP。
  主机默认特性编译仍有原有 core dead-code 警告。
- 在 HarmonyOS 模拟器 127.0.0.1:15563 覆盖安装验证。
  检查了首页、配置列表、添加连接、统计、更多、日志、关于和外观路由；
  已查看浅色首页／表单／日志／关于及深色外观／首页截图。
  日志记录开关产生的真实日志行已在新虚拟列表中渲染，并打开详情。
  最终构建的稳定详情弹窗已截图检查，系统返回键验证结果为：
  详情关闭，日志页仍保留。测试结束时关闭日志记录。
  未进行真实网关认证，因此认证弹层、证书选择和完整连接流程不属于
  本次设备端验收范围；不将编译通过等同于这些流程的视觉验收。

本地构建、测试日志和截图保存在 `smoke-logs/arkit-shadcn-*`，
不纳入版本管理。HAP 位于
`entry/build/default/outputs/default/entry-default-unsigned.hap`。
