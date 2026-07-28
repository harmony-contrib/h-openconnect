# UI 映射

## 状态机（真实 OpenConnect）

| 状态 | 含义 |
| --- | --- |
| Disconnected | 未连接 |
| Connecting / Authenticating | UI 进程 obtain_cookie + CSTP（取网络配置） |
| Establishing | 已请求系统 VPN 扩展；扩展进程 re-auth + create TUN |
| Connected | 扩展进程 mainloop 运行；UI 通过 `platform-vpn-state.json` 同步 |
| Disconnecting / Failed | 断开或失败 |

## 用户操作 → 实现

| 页面 | 用户操作 | 实现 |
| --- | --- | --- |
| Home | 连接 | `prepare_connect` → 写 `session-handoff.json` → `requestStartVpn` → 扩展 `prepareVpnInExtension` + `startVpn(fd)` |
| Home | 断开 | `requestStopVpn` → 扩展 destroy + `Command::Cancel` / platform state clear |
| Connections | 保存 | `ProfileStore`（**测试阶段写密码**到本地沙箱） |
| Statistics | 刷新 | `tick` + 共享 platform state 流量 |
| More / Diagnostics | 诊断 | `snapshot.diagnostics` + E2E 标记 |

## 入口一致性

| 入口 | dry-run | 真实隧道 |
| --- | --- | --- |
| UI 连接按钮 | 默认关（native 构建） | 是 |
| NAPI `prepareVpn` | 同 env | 是 |
| E2E Want `hanyDryRun=false` | 关 | 是 |
| E2E `hanyDryRun=true` | 开 | 仅生命周期（测试用） |
| 扩展 `prepareVpnInExtension` | — | 扩展进程只使用 UI 认证得到的 cookie 建立 CSTP |

## 持久化

| 文件 | 内容 |
| --- | --- |
| `connections.json` | 连接列表（当前测试阶段会写入密码/密钥口令，仅限应用沙箱） |
| `preferences.json` | `activeConnectionId`（选中连接跨重启） |

- 首次无 `connections.json` 时才写入 demo 种子；空文件表示用户已清空，**不会**再次注入 mock。
- 选中连接变更 / 删除后都会写 `preferences.json`。

## 动态认证表单 / 连续 challenge（已实现）

| 层 | 行为 |
| --- | --- |
| **anyconnect-rs** | `AuthForm` 多轮回调；预选 auth group 在首个 XML POST 中发送，后续切组返回 `NewGroup` 获取对应 AAA/RADIUS 表单 |
| **core `AuthInteraction`** | form handler 自动填 username/password/group；Token/SSO 字段留给 OpenConnect 生成；不足则 `wait_for_reply` 阻塞；UI `submit` / `cancel` 解锁 |
| **UI** | 连接中 250ms 轮询；`pending_auth` 时弹出底部 sheet；支持 text/password/token/select；多轮自动再弹 |
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
| **UI → ArkTS** | `pickCertFile({id,kind})` → `DocumentViewPicker` → 复制到 `filesDir/h-anyconnect/certs/` |
| **回传** | NAPI `completeFilePick(id, path)` 解锁 UI 并写入 draft |
| **OpenConnect** | 主/次客户端证书、对应私钥和 `set_ca_file` 使用沙箱绝对路径（handoff 到扩展进程） |

## UI 交互约定（测试阶段）

| 项 | 行为 |
| --- | --- |
| Challenge 输入 | 多轮表单一律**明文可见**（含 password/token 类型） |
| 连接编辑 | 常用字段常显；`显示高级选项` 开关控制非常用项；枚举用 **Select** |
| 密码存储 | 与 profile 一并写入 `connections.json`（测试用；生产需再收紧） |
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
| 主页密码 | 连接前可在主页填写密码（仅内存） |
| 允许局域网 | OHOS 无 `allowBypass` → 全隧道时把 RFC1918/link-local 标 `isExcludedRoute` |
| 按应用分流 | `trustedApplications` / `blockedApplications` 写入系统 VpnConfig |
| 主循环重连 | `reconnected_handler` 记 e2e 标记 |

后续可选：密码 Input 安全样式、内嵌 WebView SAML。
