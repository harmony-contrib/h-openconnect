# Issue #3：跨平台对照与 OHOS 最佳处理方式

## 结论

应分开建模“用户连接意图、平台 VPN 资源、协议会话、UI 显示”。
系统创建 TUN 成功、扩展进程存活、OpenConnect worker 尚未退出，都不等于协议一直可用。
最佳方案是协议与平台事件驱动的单一会话管理器，心跳用于兜底，而不是由 UI 定时器承担
所有连接管理，也不是删除排除路由来避免平台差异。

以下为 2026-09-07 的源码分析。当前修复分支已实施的内容及 QEMU 结果另见
[修复与验证记录](issue-3-analysis.md)；本文中的后续架构建议不能视为已经实现或验收。

## Android / Apple 的处理方式

Android 的 `VpnService.protect(fd)` 将隧道控制 socket 排除在 VPN 之外，避免默认路由
把隧道自身再次送回 TUN；授权撤销时 `onRevoke()` 要求应用关闭 FD 并清理。
服务端协议掉线与系统授权撤销不是同一个事件。
见 [VpnService API](https://developer.android.com/reference/android/net/VpnService)。

AOSP `VpnService` 源码把进程崩溃/被杀后的恢复与 TUN FD 生命周期关联，并将逐 FD
保护交给 `NetworkUtilsInternal.protectFromVpn`；`excludeRoute` 也使用 `RTN_THROW`。
这与 OHOS 的平台资源和路由机制相通，但不意味着系统能替用户态协议判断服务器掉线。
见 [AOSP VpnService.java](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/net/VpnService.java)。

Android always-on 由系统维护服务生命周期，但 VPN 服务仍负责网关连接；阻断所有
非 VPN 流量是单独的 lockdown 策略，不等于“全路由”或“禁止 LAN 绕行”。
见 [Android VPN 指南](https://developer.android.com/develop/connectivity/vpn)。

Android API 33 起提供 `excludeRoute`，路由按最长前缀匹配；相同前缀的 add/exclude
由最后调用覆盖。`allowBypass()` 是允许应用主动选择其他网络，不是 LAN 路由开关。
我们的路由编译器采用“相同前缀排除优先”的显式产品策略，不能宣称完全复制 Android 的
调用顺序语义。见 [VpnService.Builder](https://developer.android.com/reference/android/net/VpnService.Builder)。

Apple 的 tunnel provider 有独立的启动完成确认和 `reasserting` 状态：开始重连时
设置后者，恢复后清除。这说明“正在恢复”不应继续被 UI 表述为正常在线。
见 [启动完成语义](https://developer.apple.com/documentation/networkextension/nepackettunnelprovider/starttunnel(options:completionhandler:))
与 [reasserting](https://developer.apple.com/documentation/networkextension/netunnelprovider/reasserting)。

## 本地 OHOS 源码的确切含义

下列路径相对于 OpenHarmony 源码树的 `foundation/communication` 目录。

- `netmanager_ext` HEAD：`2de6cf12b48e31d364474adf4f23884758d718c8`
- `netmanager_base` HEAD：`0e81c0a6a7549b27f31ef6cb643925b11f293e08`

以上是检查时的源码版本，不应等同于所有 HarmonyOS 商用固件，也不证明镜像每个二进制
都来自未修改的相同提交。设备数据路径仍须用 QEMU/真机单独验证。

| 源码位置 | 可确认的行为 | 对本应用的含义 |
| --- | --- | --- |
| `netmanager_ext/services/vpnmanager/src/net_vpn_impl.cpp`，`SetUp` | 更新地址/路由/DNS、安装 UID 策略后发出 `VPN_CONNECTED` | 仅表示平台网络已设置，不知道用户态 OpenConnect 的认证、CSTP、DPD 是否正常 |
| 同文件，`Destroy` | 删除 UID 策略、网络链路信息、supplier，通知断开 | 应执行平台 destroy；只改 UI 或关闭协议 socket 不能等价替代 |
| `netmanager_ext/services/vpnmanager/src/networkvpn_service.cpp`，`VpnHapObserver::OnProcessDied` / `OnRemoteDied` | 对匹配的创建进程死亡、回调远端死亡进行 VPN 清理；部分路径还会停止扩展 | 进程死亡有系统兜底，但同进程里一个 C worker 退出不会触发该观察者；当前实现还可能关联主应用进程，不能承诺杀掉主进程仍保持隧道 |
| `netmanager_ext/frameworks/native/netvpnclient/src/networkvpn_client.cpp`，`DestroyVpn` | 先关闭客户端 TUN FD、注销回调，再通过服务代理销毁 VPN | 清理不是单个 JS 对象的内存释放；classic 路径存在进程/服务级共享状态，旧 destroy 必须完成后才能创建新会话 |
| 同文件，`Protect`；`netmanager_base/.../netsys/fwmark_network.cpp` | 通过 `ProtectFromVpn` 设置 socket 的 `protectedFromVpn` 标记 | 控制 socket 使用正式 protect API，不靠添加服务器 IP 绕行路由代替，也不手动设置未经授权的 fwmark |
| `netmanager_ext/frameworks/js/napi/vpnext/src/context/setup_context_ext.cpp`，`ParseRoute` | classic `create` 路径确实读取 `isExcludedRoute` | “OHOS 完全不支持排除路由”不是本地源码事实 |
| `netmanager_base/services/netmanagernative/src/manager/route_manager.cpp`，`UpdateRouteRule` | 排除路由下发为 `RTN_THROW` | 排除是退出当前表继续查找，不是把该网段送回 TUN |
| `netmanager_ext/services/vpnmanager/src/net_vpn_impl.cpp`，`UpdateNetLinkInfo` | 空 routes 按允许的地址族补默认路由 | 排除计算结果为空时必须明确拒绝，不能交空数组表达“没有 VPN 路由” |

### 不应直接使用源码里的隐藏接口

`vpn_module_ext.cpp` 注册了 `on/off`，`vpn_monitor_ext.cpp` 把 `connect` 事件转成
布尔值；但本地 `interface/sdk-js/api/@ohos.net.vpnExtension.d.ts` 与当前 DevEco SDK
的公开 `VpnConnection` 都没有声明该接口。因此源码中存在不等于应用可稳定依赖。
即使在特定镜像中调用成功，该布尔值也没有会话 ID，且只是平台网络状态。

公开 `connection.NetConnection` 有 `netAvailable/netLost` 等事件，可用于观察底层
网络以及辅助核对 VPN 网络消失。需要确认权限、运行时支持并关联本次 NetHandle/会话；
任意网络的 `netLost` 不能直接解释为本次 VPN 失败。

公开声明还明确：`protectProcessNet()` 从 API 22 起提供，只覆盖本进程随后创建的
socket，不补保护既有 socket。面向 API 20 的兼容方案仍须保留逐 FD protect，且必须
在扩展进程调用进程级保护，不能让普通 UI/业务进程全部绕开 VPN。

此外，源码声明中的 `setAlwaysOnVpn` 是需要 `MANAGE_VPN` 的 system API，普通应用
不能照搬系统权限调用。后台常驻能力必须结合目标系统公开能力与用户授权评估。

## 推荐的会话管理方式

1. **单一运行期所有者。** 扩展内的 session controller 管理 native worker、TUN、
   protect 注册、清理与重试。UI 管理用户意图与交互认证，显示会话快照，不持有第二份
   “正在运行”的连接对象。初始认证 handoff 完成后释放 UI 准备态。
2. **明确状态而非组合猜测。** 区分 Authorizing、Preparing、PlatformReady、Connected、
   Reconnecting、Stopping 和终态。`extensionAttached` 是里程碑而不是终态；首次接管
   的 Pending 中间帧不应取消启动。Connected 至少要求本次 TUN 已创建、协议已准备并
   挂接成功。所有结果带 attempt ID 和 generation，旧会话不能恢复新状态。
   新 Want 的合法性必须在任何 destroy 或 IPC 替换之前核对；“先清理、后发现 Want
   已过期”同样会误伤新会话，不能只按系统回调的到达顺序判断请求新旧。
3. **事件优先，活性检测兜底。** native 返回/错误立即进入状态机；平台停止及经过关联的
   网络事件补充信息。远端 IPC 更新停滞才由单调时钟 watchdog 兜底，并有唤醒宽限。
   当前通知传输是 UNIX `SOCK_DGRAM`，不是天然具有可靠对端 EOF 的 stream；不能
   假设现有 IPC 已提供进程死亡通知。若需要更及时的对端死亡判定，可另行设计有明确
   所有权的 RPC death notification 或 stream 监督通道。
4. **区分进程活性与链路健康。** 不用“流量没有增长”判断断链，也不依赖 ping 公共网站。
   DPD 和协议控制通道负责检测对端。OpenConnect 本身会在临时掉线后内部重连；这段
   时间 worker 和扩展心跳可能都正常，但 UI 应显示 Reconnecting。
   见 [OpenConnect 重连与 DPD 说明](https://www.infradead.org/openconnect/manual.html)。
5. **串行且可归属的清理。** 先阻止本次请求继续生效，取消 worker，再等待本次原始
   平台操作及 destroy 完成，最后释放资源并允许下一次 create。Promise 超时只是
   等待超时，不是取消系统操作。超时后 detach 的 C 线程也不等于已经退出。
6. **重试遵守用户意图。** 临时网络故障可由唯一控制器退避重试；用户断开、系统撤销
   授权、凭据/MFA 需要交互时不能自动循环重启。后台恢复应依赖平台支持的生命周期及
   安全认证设计，不应依赖 UI 的定时器或将明文 cookie 写盘来延长会话。

QEMU 的 `issue3-2in1-matrix-fix2` 暴露了第 5 点的具体反例：旧 `stop_vpn` 等待
`disconnect()` 的 200 毫秒期间，UI 发布了新 attempt；旧 stop 随后的全局
`set_platform_vpn_running(false)` 又先同步并接管新 attempt，最终取消新连接。
所以仅在 ArkTS 排队还不够：Rust 核心的接管操作必须发生在明确的新请求绑定点，
异步清理完成也必须校验原 attempt/generation，不能先同步新 owner 再执行旧 stop。

当前库 `anyconnect 0.1.1` 已暴露 `reconnected_handler`，但没有对应的结构化
“开始重连”回调。本分支对 mainloop 终止和扩展冻结进行了修复；精确呈现 native
内部 300 秒重连窗口，需要补充结构化状态信号并另行验证，不能解析英文日志充当协议。

## 路由的最佳取舍

对于版本受控、排除链路已经设备验证的 OHOS 系统，原生 `isExcludedRoute` 最直接，
路由条目也更少。对于需兼容不同厂商/API 版本的应用，建议保留统一路由策略模型，
将其编译为等价正向 CIDR 作为兼容路径。不能只凭 JS 接受字段就断定内核已实现排除；
静默忽略字段必须通过受控数据路径测试才能识别。

当前分支使用正向 CIDR 的目的为跨版本确定性，不是声称 OHOS 无排除能力。必须保留
最长前缀、更具体的私有 DNS `/32` 或 `/128`、服务端排除和用户 LAN 偏好，并拒绝
空结果。LAN 开关、应用是否允许主动绕过 VPN、隧道控制 socket 的 protect 是三个
独立概念。

失败后的“恢复普通联网”与“阻断所有非 VPN 流量”是另一项安全策略选择。本轮沿用应用
原有清理语义，没有擅自增加 lockdown/kill switch，也不把保留一个死 TUN 当成可靠
的防泄漏实现。

## 如果同时维护 OHOS 系统源码

平台层更适合做以下改进，但这些是系统演进建议，不是本分支已经修改或验证的功能：

- 统一 classic 与其他 VPN 配置解析路径的路由字段及序列化契约，并覆盖排除路由的
  端到端测试，避免同一字段在不同入口中静默丢失。
- 对外提供稳定、有文档的网络生命周期事件，携带会话标识和停止原因；清楚区分
  “平台网络安装完成”和应用报告的“协议就绪”，不假装系统能自动理解第三方协议。
- 将 create/destroy/recovery 都绑定到明确的资源 owner 和版本；异步销毁或系统服务
  重启后的恢复必须重新核对 desired state，不能对下一次会话应用旧请求。
- 对进程死亡、服务重启、TUN FD 关闭、路由/UID 策略移除做联合故障测试；验证内核
  资源回收，而不仅验证系统接口返回成功。

应用仍必须自行负责协议终态与 DPD。当前没有为绕过应用竞态而修改本地 OHOS 源码
或原始 QEMU 镜像；在原镜像的干净副本上验证应用修复，才有可比较的结果。
