# 认证能力与 QEMU 验证

本文记录 ARM64 2-in-1 standard-system QEMU（本地 2026-09-07 构建）上的真实操作结果。
“通过”表示使用正式签名 HAP，经 UI 认证、跨进程 handoff、VpnExtension、CSTP、系统
TUN、应用 UID DNS/TCP、断开和重连的完整链路；它不只是检查登录 cookie。

## 结论

| 能力 | 当前结论 | 验证层级 |
| --- | --- | --- |
| 用户名/密码 | 支持 | QEMU 完整链路 |
| PEM 证书和独立私钥 | 支持 | QEMU 完整链路，ocserv 证书认证 |
| 加密 PKCS#12 + 密码 | 支持 | QEMU 完整链路，密码与证书双认证 |
| SAML / Cisco SSO-v2 | 支持，但设备必须有外部浏览器 Ability | QEMU 完成 HPKE、loopback callback、cookie、CSTP 和数据链路 |
| 系统无浏览器或 `openLink` 拒绝 | 可控失败 | QEMU 验证不再无限等待 loopback callback |
| 服务端 OTP / RADIUS challenge | 支持 | 表单状态机和 host E2E；尚无 QEMU OTP 服务端实测 |
| 内置 TOTP | 支持 | OpenConnect 构建和 host 测试；尚无 QEMU TOTP 服务端实测 |
| 第二客户端证书 | 有协议实现 | OpenConnect upstream E2E；尚无 QEMU 双证书服务端实测 |
| RSA SecurID / stoken | 不支持 | ARM64 产物未链接 libstoken；UI 不再展示，旧配置会明确报错 |
| PKCS#11、P11-kit、TPM/TSS2 | 不支持 | ARM64 产物未链接对应 provider；证书字段只接受沙箱文件路径 |
| FIPS | 不支持 | 没有经验证的 OpenSSL FIPS provider；连接前明确拒绝 |
| CSD wrapper | 不承诺移动端支持 | 字段保留兼容性，不作为 HarmonyOS 生产能力声明 |

因此，当前不能笼统声称支持 OpenConnect 的全部认证后端。已经实际证明的生产范围是
密码、文件证书、密码加文件证书以及有可用系统浏览器时的 SSO-v2。TOTP、双证书仍需
对应企业网关或专用 QEMU 认证服务补充设备级验收。

## SAML 缺陷与处理

OpenConnect 会先监听 `http://[::1]:29786`，再调用平台回调打开 IdP。上游在浏览器
回调失败时只记录错误，仍会继续等待 loopback 连接。裸 QEMU 镜像没有处理 HTTP URL
的浏览器 Ability，系统返回 `2097199`；原实现又把 `openLink` 当作 fire-and-forget，
最终表现为连接永远停在认证中。

修复后的 UI 认证路径等待 `openLink` 的实际结果。系统拒绝或十秒内没有确认时，回调
返回失败并向 OpenConnect command pipe 写入 `Cancel`，使其 `cancellable_accept`
立即退出。已注册的 UI handler 返回失败时也不会再错误地降级为 extension ashmem
请求。应用不会用内嵌 WebView 冒充外部浏览器：企业 IdP 往往依赖系统 SSO cookie、
通行密钥或浏览器安全策略，静默降级会改变认证安全边界。

QEMU 没有浏览器并不表示 SSO-v2 协议不工作。测试工具以设备侧外部导航器完成真实
HTTP 重定向和 IPv6 loopback callback，已验证 P-256 ECDH、HKDF-SHA256、
AES-256-GCM、SSO token、CSTP 和隧道数据。另一路系统驱动测试专门验证缺少浏览器
时能快速失败且不创建 TUN。

## 证据与复现

本地证据默认位于以下相对目录（`smoke-logs/` 不纳入版本控制）：

- `smoke-logs/issue3-2in1-matrix-fix3/`：密码、路由和生命周期。
- `smoke-logs/auth-cert-pem-final-20260908/`：PEM 证书。
- `smoke-logs/auth-password-cert-p12-20260908/`：加密 PKCS#12 双认证。
- `smoke-logs/auth-saml-sso-v2-complete-20260908/`：SSO-v2 与完整数据链路。
- `smoke-logs/auth-saml-sso-v2-browser-failure-20260908/`：系统浏览器拒绝与取消。

复现命令、环境变量和断言见 `docs/e2e.md`。验证时应同时检查 UI 终态、`vpn-tun`、
系统路由、ocserv 在线用户、应用 UID 的 DNS/TCP，以及
`openconnect-progress.log`；仅看到认证成功不能证明系统数据链路可用。

## 超时策略

`withTimeout` 需要保留。它约束的是 ArkTS 与系统之间无法保证自行结束的 Promise，
包括 extension prepare、TUN create、native attach、mainloop 和 cleanup。超时负责让
attempt 进入确定终态，但不能代替底层取消和资源回收。

SAML 浏览器启动属于认证前的另一边界，使用独立的十秒确认与 OpenConnect command
pipe 取消。这样既不会因为删除 `withTimeout` 重新引入系统调用永久 pending，也不会
让平台操作超时承担浏览器回调的职责。
