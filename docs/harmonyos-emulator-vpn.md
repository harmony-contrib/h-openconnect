# HarmonyOS 模拟器 VPN 接入标准

本文定义 H-OpenConnect 在 DevEco HarmonyOS 模拟器上启动真实
`VpnExtensionAbility`、创建系统 TUN 并同步应用状态的标准实现和验收方法。该方案不
修改系统镜像、不复制系统私有 `.so`、不 mock VPN，也不以单元测试代替真实隧道。

## 适用范围

- H-OpenConnect debug HAP；
- 缺少系统 VPN 授权弹窗的 DevEco 模拟器镜像；
- 系统能提供 VPN Manager、`VpnExtensionAbility` 和 `/dev/tun`，但不能可靠转发首个
  携带 FD 的 VPN Want；
- 真机和完整 standard-system 镜像仍使用相同的真实 VPN/TUN 数据链路，但 release HAP
  不启用模拟器授权兼容逻辑。

当前验证基线为 DevEco Studio 26.0.0.821、target API 24、compatible API 22。其他镜像
必须按本文的验收项重新验证，不能只以 `startVpnExtensionAbility()` Promise resolve
作为成功依据。

## 已确认的镜像差异

### 缺少授权 UI

受影响镜像没有可连接的
`com.huawei.hmos.vpndialog/VpnServiceExtAbility`。首次启动常见日志为：

```text
StartVpnExtensionAbility
dataShareHelperUtils Query error, err = -1
connectAbility failed 2097152
```

这表示系统授权交互链路缺失，不表示应用的 `VpnExtensionAbility` 名称错误。

### 首个 FD Want 被静默丢弃

系统可能成功返回 `startVpnExtensionAbility()`，同时打印 `WriteToParcelFD`，但不创建
应用的 `:vpn` 进程，也不回调 Extension。第二次调用 VPN start 即使再次 resolve，也
不会可靠触发已启动 Extension 的 `onRequest`。

因此不能把 ashmem FD 或通知 FD 放入用于创建 VPN Extension 的第一个 Want，也不能
用“延迟后再次调用 VPN start”作为补偿。

### 系统镜像不能直接修改

直接修改镜像中的白名单、系统配置或系统应用会破坏镜像文件校验。复制系统内部
依赖库到 HAP 还会引入命名空间和 ABI 问题，例如 `libzuri.z.so`、
`libnet_data_share.z.so` 或 `libc++.so` 的级联加载失败；即使加载成功，也不能因此获得
系统服务进程中的同一份状态。

这些方式不属于本项目的支持方案。

## 标准启动流程

```text
UI 进程                               VPN Extension 进程
   │
   ├─ 创建 ashmem + 通知通道
   ├─ debug：更新当前 bundle 的 VPN 授权状态
   ├─ 创建 attempt 和一次性 128-bit handoff token
   ├─ 监听 token 对应的 AF_UNIX abstract socket
   ├─ startVpnExtensionAbility(无 FD Want) ─────────► onCreate
   │                                                  ├─ 配置 native home
   │                                                  └─ 使用 token 连接 handoff socket
   ├─ accept
   ├─ SCM_RIGHTS 转移 ashmem/通知 FD ───────────────► 校验 owner journal + ashmem attempt
   │                                                  ├─ attach 同一块 ashmem
   │                                                  ├─ bind attempt
   │                                                  ├─ CSTP/network_config
   │                                                  ├─ VpnConnection.create
   │                                                  └─ OpenConnect packet loop
   ◄═══════════════ 同一块 ashmem 中的状态/统计 ══════┘
```

关键约束：

1. 每个用户连接意图只调用一次系统 VPN start；该 Want 只携带 attempt ID 和一次性
   token，不携带 FD、cookie 或凭据。
2. FD 由内核 `SCM_RIGHTS` 转移，不复制共享内存内容。UI 和 VPN 进程继续访问同一块
   ashmem，状态更新路径与真机一致。
3. token 从 `/dev/urandom` 生成，监听器只消费一次；接收的 FD 设置 `FD_CLOEXEC`。
4. VPN 进程在 attach 前校验持久化 owner journal、attempt ID 和 ashmem 中的 UI 状态。
   过期、串线或已停止的请求不能接管隧道。
5. cookie、认证结果和运行时状态只存在于 ashmem，不写入 handoff 文件。

## 模拟器授权策略

系统授权弹窗缺失时，debug HAP 在插件安装阶段调用：

```text
updateVpnAuthorizedState(<current bundle name>)
```

实现必须同时满足：

- 仅在 `BuildProfile.DEBUG` 为真时执行；
- bundle name 来自当前构建配置，不能硬编码其他应用；
- release HAP 不调用隐藏 API，真机继续由系统授权 UI 管理；
- 日志中同时核对应用返回值和系统 `UpdateVpnAuthorize result. ret = 0`；
- 应用启动时只恢复本应用留下的精确 stale attempt，不清理其他应用或新 attempt。

相关代码集中在
`entry/src/main/ets/vpnability/VpnEmulatorCompatibility.ets`，隐藏 API 的本地声明位于
`entry/src/main/ets/types/vpnExtensionDebug.d.ts`。不得把这段逻辑扩展到 release。

## 代码职责

| 文件 | 职责 |
| --- | --- |
| `VpnPlugin.ets` | 用户 start/stop 意图、串行化、终态等待和精确清理 |
| `VpnEmulatorCompatibility.ets` | debug 授权和模拟器 stale owner 恢复边界 |
| `VpnExtensionLauncher.ets` | 单次无 FD 系统启动及一次性 FD handoff |
| `VpnConfig.ets` | VPN Want、参数读取和 `VpnConfig` 映射 |
| `HOpenConnectVpnExtensionAbility.ets` | 接收 handoff、绑定 owner、创建 TUN 和启动数据面 |
| `vpn_handoff.rs` | AF_UNIX、随机 token、`SCM_RIGHTS` 和 FD 所有权 |
| `platform_ipc.rs` | ashmem 双 lane 状态帧及变更通知 |

## 构建与安装

必须完整重编 native `.so`。只运行 Hvigor 可能复用旧的
`entry/libs/arm64-v8a/libhopenconnect_ui.so`，导致 ArkTS 已更新而运行时找不到新增 N-API。

```bash
: "${DEVECO_STUDIO_HOME:?设置为 DevEco Studio 的 Contents 目录}"
export DEVECO_SDK_HOME="$DEVECO_STUDIO_HOME/sdk"
export OHOS_NDK_HOME="$DEVECO_SDK_HOME/default/openharmony"

SIGN_HAP=1 HAP_BUILD_MODE=debug ./scripts/package-hap.sh

export HDC_TARGET=127.0.0.1:5555
hdc -t "$HDC_TARGET" install -r \
  entry/build/default/outputs/default/entry-default-signed.hap
hdc -t "$HDC_TARGET" shell hilog -b I -T HOpenConnectVpn
```

如果只为排查 ArkTS 编译而跳过 native 构建，该产物不能用于本流程验收。

## 验收标准

由测试人员在 UI 中创建真实配置并主动连接。自动化工具不得替用户点击连接按钮。

### 必须出现的阶段日志

```text
initialized platform shared memory
debug updateVpnAuthorizedState returned true; verify system ret is 0
dispatching descriptor-free system VPN bootstrap attempt ...
system VPN bootstrap accepted attempt ...
onCreate
waiting for VPN session FD handoff attempt ...
sending VPN session FD handoff attempt ...
received and attached VPN session FD handoff attempt ...
bound VPN owner attempt ...
extension prepare ok ...
VPN TUN fd=...
protectProcessNet ok
VPN start completed attempt ...
```

### 必须满足的系统状态

```bash
hdc -t "$HDC_TARGET" shell ps -A | grep 'h_openconnect\|openconnect:vpn'
hdc -t "$HDC_TARGET" shell ifconfig vpn-tun
hdc -t "$HDC_TARGET" shell aa dump -a | grep HOpenConnectVpnExtensionAbility
```

- 主应用进程和 `com.richerfu.h_openconnect:vpn` 同时存在；
- `vpn-tun` 存在且地址与 headend 下发配置一致；
- UI 从“正在建立隧道”进入“已连接”，流量统计能继续变化；
- 断开后 TUN 消失，重新连接产生新的 attempt 并再次成功；
- 应用 UID 的 DNS/TCP 流量按服务端路由验证。root `hdc shell` 流量不作为 VPN
  路由证据。

## 故障定位

| 现象 | 判定 | 处理 |
| --- | --- | --- |
| `connectAbility failed 2097152` | 镜像缺少 VPN 授权 UI | 确认 debug 授权日志和系统 `ret = 0` |
| start resolve，但没有 `:vpn`/`onCreate` | 首个 Want 仍携带 FD，或授权未生效 | 检查 `VpnExtensionLauncher` 只发一个无 FD Want |
| `:vpn` 已启动，但没有 `received and attached` | 一次性 socket/FD handoff 失败 | 检查 native `.so` 是否完整重编及 handoff 两端日志 |
| 已 attach，但 UI 状态不变 | attempt 校验、ashmem 通知或 owner 绑定失败 | 检查 `bound VPN owner`、terminal state 和通知通道 |
| 有 owner，无 `vpn-tun` | CSTP/network config 或 `VpnConnection.create` 失败 | 检查 `extension prepare`、地址、路由、DNS 和 create 错误 |
| `libzuri.z.so` 等加载失败 | 错误复制/链接系统私有库 | 删除私有系统库依赖，恢复本项目 native 构建 |
| 修改镜像后启动校验失败 | 系统镜像完整性被破坏 | 恢复原镜像；不要把镜像修改作为应用接入步骤 |

## 自动检查

```bash
cargo test -p hopenconnect_ui --test vpn_startup_contract
git diff --check
```

契约测试会检查：单次无 FD 系统启动、随机 token、`SCM_RIGHTS`、Extension attach/bind
顺序、debug-only 授权边界，以及 stop/cleanup 的 attempt 隔离。最终结论仍以真实
Extension、真实 `vpn-tun` 和应用 UID 流量为准。
