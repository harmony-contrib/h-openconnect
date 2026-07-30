# UI 映射

## 状态机（真实 OpenConnect）

| 状态 | 含义 |
| --- | --- |
| Disconnected | 未连接 |
| Connecting / Authenticating | UI 进程 obtain_cookie + CSTP（取网络配置） |
| Establishing | 已请求系统 VPN 扩展；扩展进程 re-auth + create TUN |
| Connected | 扩展进程 mainloop 运行；UI 通过 ashmem 状态帧与变更通知同步 |
| Disconnecting / Failed | 断开或失败 |

## 用户操作 → 实现

| 页面 | 用户操作 | 实现 |
| --- | --- | --- |
| Home | 连接 | `prepare_connect` → 写 `session-handoff.json` → `requestStartVpn` → 扩展 `prepareVpnInExtension` + `startVpn(fd)` |
| Home | 断开 | `requestStopVpn` → 扩展 destroy + `Command::Cancel` / platform state clear |
| Connections | 保存 | `ProfileStore`（密码持久化在禁止备份、权限收紧的应用私有目录） |
| Statistics | 刷新 | `tick` + 共享 platform state 流量 |
| More / Logs | 与 Paws 一致的当前/历史分段、关键词与等级筛选、点击查看全文、开始/停止记录、导出/删除历史 | 默认关闭；当前日志使用虚拟列表，开启后内存保留 256 条、突发队列最多 4096 条，按 UTC 日期写入应用私有目录；活动归档停止记录后才可删除，导出走系统文档选择器 |
| More / About | 开源组件、许可、隐私与源码链接 | 展示应用/引擎版本、MIT OR Apache-2.0 双许可、OpenConnect LGPL-2.1-only 归属、主要组件源码入口及本地数据处理说明 |

## 入口一致性

| 入口 | dry-run | 真实隧道 |
| --- | --- | --- |
| UI 连接按钮 | 默认关（native 构建） | 是 |
| NAPI `prepareVpn` | 同 env | 是 |
| 扩展 `prepareVpnInExtension` | — | 扩展进程只使用 UI 认证得到的 cookie 建立 CSTP |

## 持久化

| 文件 | 内容 |
| --- | --- |
| `connections.json` | 连接列表与持久化凭据；应用私有、`0600`、禁止系统备份 |
| `preferences.json` | 选中连接、语言与主题 |

- 首次启动创建空配置文件，不向正式应用注入 demo 连接。
- 选中连接变更 / 删除后都会写 `preferences.json`。

## 动态认证表单 / 连续 challenge（已实现）

| 层 | 行为 |
| --- | --- |
| **anyconnect-rs** | `AuthForm` 多轮回调；预选 auth group 在首个 XML POST 中发送，后续切组返回 `NewGroup` 获取对应 AAA/RADIUS 表单 |
| **core `AuthInteraction`** | form handler 自动填 username/password/group；Token/SSO 字段留给 OpenConnect 生成；不足则 `wait_for_reply` 阻塞；UI `submit` / `cancel` 解锁 |
| **UI** | 连接中使用最长 250ms 的事件等待；`pending_auth` 时弹出底部 sheet；支持 text/password/token/select；多轮自动再弹 |
| **VPN 扩展** | 非交互 autofill（密码/cookie re-auth）；不弹 UI |

NAPI：`pending_auth_challenge` / `submit_auth_challenge` / `cancel_auth_challenge`。

服务器地址输入停止 700ms 后，UI 会用不携带已保存 group、且禁止外部浏览器
SSO 的首次请求读取服务器分组；成功后使用服务器协议值填充选择框。

## SAML 外部浏览器（已实现）

| 层 | 行为 |
| --- | --- |
| **Profile** | `auth_method=Saml` 默认 `external_browser_auth=true`；开关可关 |
| **OpenConnect** | SSO-v2：监听 `http://[::1]:29786/api/sso/…`，再回调 `external_browser_handler` |
| **UI 进程** | 完成 SSO 与所有交互认证，得到 cookie 后再启动 VpnExtension |
| **UI 轮询** | `TickSession` → `take_browser_open_pending` → `openExternalBrowser` |
| **UI → ArkTS** | `openExternalBrowser(uri)` → `Want` `ohos.want.action.viewData` 打开系统浏览器 |
| **用户** | 连接后 toast；在系统浏览器完成 IdP 登录，浏览器回跳本地端口完成 cookie |

## 证书文件选择（已实现）

| 层 | 行为 |
| --- | --- |
| **连接编辑** | 客户端证书 / 私钥 / CA 路径旁「选择文件」 |
| **UI → ArkTS** | `pickCertFile({id,kind})` → `DocumentViewPicker` → 复制到 `filesDir/h-openconnect/certs/` |
| **回传** | NAPI `completeFilePick(id, path)` 解锁 UI 并写入 draft |
| **OpenConnect** | 主/次客户端证书、对应私钥和 `set_ca_file` 使用沙箱绝对路径（handoff 到扩展进程） |

## UI 交互约定（测试阶段）

| 项 | 行为 |
| --- | --- |
| Challenge 输入 | 多轮表单一律**明文可见**（含 password/token 类型） |
| 连接编辑 | 常用字段常显；`显示高级选项` 开关控制非常用项；枚举用 **Select** |
| 密码存储 | 与 profile 一并写入应用私有目录的 `connections.json`，文件权限 `0600`，目录权限 `0700`，并禁止系统备份 |
| Toast | 仅关键状态/错误；**顶部**、最多 2 条、短时消失 |

## 本轮对齐（ics / OpenConnect / OHOS 系统 VPN）

| 能力 | 实现 |
| --- | --- |
| PKCS#12 / PEM 口令 | `key_password` / `secondary_key_password` → 对应 OpenConnect setter，并随扩展 handoff 传递 |
| HTTP 代理 | `http_proxy` → `set_http_proxy` |
| 服务器证书钉扎 | `server_cert_hash`（`--servercert` / pin-sha256）优先于宽松信任 |
| 外部认证通告 | 未启用浏览器 SSO 时使用 `--no-external-auth` 等价语义 |
| 备份网关 | 主网关仅在网络/TLS/连接失败时按配置顺序故障转移，不重试密码或用户取消 |
| 服务端 XML profile | 原子写入应用沙箱中的 `anyconnect-server-profile.xml` |
| 主页密码 | 连接前可填写并随 profile 持久化，不再使用“仅本次”弹窗 |
| 允许局域网 | OHOS 无 `allowBypass` → 全隧道时把 RFC1918/link-local 标 `isExcludedRoute` |
| 按应用分流 | `trustedApplications` / `blockedApplications` 写入系统 VpnConfig |
| 主循环重连 | 使用 OpenConnect 主循环的标准重连机制；App 通过 ashmem 通知与有界等待同步平台状态，不写测试标记 |

后续可选：通过设备密钥服务加密静态凭据、内嵌 WebView SAML。
