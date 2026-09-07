# Issue #3：路由与会话生命周期修复

本文对应 [issue #3](https://github.com/harmony-contrib/h-openconnect/issues/3) 及其
[附件报告](https://github.com/user-attachments/files/31876481/ISSUE_REPORT.md)。
分析基线为 `main@6e213b1`，修复分支为 `fix/issue-3-vpn-routing-lifecycle`。
实现与 QEMU 验证交由用户指定的 **GPT-5.6 sol / xhigh** 执行，主线程进行独立源码审查、
路由语义测试与结果复核。

Android/Apple 对照、OHOS 系统职责以及后续架构取舍见
[跨平台与 OHOS 设计分析](issue-3-platform-design.md)。

## 1. 核实结果

附件对症状的描述具有参考价值，但其建议不能整体覆盖上游代码。以下区分源码可确认的
缺陷、平台版本差异和未被证实的推断。

| 项目 | 基线证据与结论 | 修复方向 |
| --- | --- | --- |
| 全路由下 LAN 偏好丢失 | `VpnOptions::from_network` 将 `allow_bypass` 设为 `allow_local_lan && !use_default`；扩展恢复又用它反推 `allow_local_lan`，因此全路由时信息丢失 | 独立传递 `allowLocalLan`，保留旧 handoff 的兼容读取 |
| 排除路由变为包含路由 | **不能概括为所有 OHOS 均不支持**。本地新镜像源码的 classic `vpnExtension.VpnConnection.create` 路径读取 `isExcludedRoute`，并最终下发 `RTN_THROW`。另一条系统 VPN 配置解析路径未读取该字段 | 将策略转换为等价正向 CIDR，避免依赖版本不一致的排除字段；不直接丢弃排除策略 |
| DNS 主机路由膨胀 | 基线 `from_network`、`apply_force_global` 和恢复路径均已检查重复；附件所述“多次恢复必然膨胀”未获源码支持 | 集中生成、规范化和去重，增加边界测试；属于维护与正确性加固 |
| 套接字保护持锁 | `platform_protect::invoke` 在注册表锁内调用等待 ArkTS Promise 的 handler；清理、替换 handler 会被阻塞 | 锁内取得回调引用，锁外调用；每次注册独立保存结果 |
| 套接字保护错误丢失 | ArkTS 注册失败仅告警；Rust 闭包通过 `let _ = protect_socket_fd(fd)` 丢弃错误 | 注册失败中止启动；将保护失败传入会话处理，并覆盖后续原生重连 |
| 主循环退出不通知 | `on_native_session_ended` 没有调用点；扩展订阅只等待控制事件，未驱动 `tick` | 主循环直接上报终态，扩展定时驱动统计和健康检查 |
| join 无实际超时 | `RunningNativeSession::join(timeout)` 丢弃 timeout 后直接阻塞 join | 有界等待，超时隔离旧 worker；禁止旧回调影响新会话 |
| UI 持续显示旧在线帧 | `sync_platform_locked` 只按时间戳去重，没有扩展活性检查；本地发布与远端去重还共用 revision 字段 | 分开两条 lane 的 revision，用本地单调时间判断扩展更新停滞 |
| 自动重连漏触发 | UI 先判断重连，再执行可能发现故障的 `tick`，随后把 `last_lifecycle` 覆写为终态 | 先检查和同步状态，再统一判断状态边沿 |
| 自动重连被旧准备态拒绝 | QEMU 踢线后发现：UI 的 `prepare_native` 保存 `pending_native`，扩展已接管并连通后该副本仍保留；新 `prepare_connect` 会因 `native session already active` 拒绝重连，而手动断开会清除它 | 仅在 UI lane 收到匹配的扩展确认后释放旧认证准备态；不能误清扩展待挂接的 TUN 会话；增加 native-feature 回归并重跑设备踢线 |
| 真实统计被模拟流量覆盖 | UI 进程正常没有 `running_native`，但基线仍对活动会话累加模拟流量；远端计数只有更大时才被采用 | 模拟计数仅限明确 dry-run；真实连接读取扩展计数 |
| 旧状态复活与异步清理竞态 | 旧 running 帧可将终态恢复为 Connected；有 TUN 时 rebind 无健康检查；`destroyVpn` 不等待销毁 Promise | 会话/请求归属检查、终态不可逆、串行清理与重建、订阅取消退避 |
| 迟到的启动帧 | 仅限制 running 晋级仍不够：同一 attempt 的迟到 `starting=true/Pending` 帧可把 Cancelled/Disconnected 改回 Establishing | starting 也只允许本地 Pending 且处于连接阶段；用完整 IPC envelope 验证 |
| 首次接管的中性帧误判 | QEMU 复跑发现：扩展接管时可能先发布 `extension_attached=true/Pending/Disconnected`；UI 把“尚未开始”当成断开，此后的正常 Connected 又被终态保护拒绝 | 扩展接管新 Pending 请求时初始化 Establishing；UI 区分 Pending 中间帧和明确终态，覆盖接管→启动→连通顺序 |
| 旧清理接管并取消新会话 | QEMU 复跑发现：旧 `stop_vpn` 等待后，全局设置 running=false 的同步会先采纳 UI 新 attempt，再误取消它 | 普通同步不得接管；显式 Want 绑定前验证归属；stop 捕获旧 owner，完成时再核对 generation/attempt；过期 Want 在任何销毁或 IPC 替换前拒绝 |
| 依赖生成物入库 | `git ls-files -s entry/oh_modules` 确认两个 `@ohos-rs` 链接以 `120000` 模式受跟踪 | 取消生成链接的 Git 跟踪并忽略该目录，保留磁盘上的已安装依赖 |

平台源码核对范围：

- `netmanager_ext/frameworks/js/napi/vpnext/src/context/setup_context_ext.cpp`：
  `SetUpContext::ParseVpnConfig` 的 classic 分支进入本文件 `ParseRoute`，读取排除标志。
- `netmanager_ext/frameworks/js/napi/vpnext/src/vpn_config_utils_ext.cpp`：
  另一解析函数没有读取相同字段，不能据此推断本应用实际执行路径。
- `netmanager_base/services/netmanagernative/src/manager/route_manager.cpp`：
  将排除标志转换为 Linux `RTN_THROW`。
- `netmanager_ext/services/vpnmanager/src/net_vpn_impl.cpp`：
  空路由列表会触发默认路由生成，因此“排除后为空”必须显式失败。

以上路径相对于 OpenHarmony 源码树的 `foundation/communication` 目录。
这不证明附件作者所用 Mate 80 / HarmonyOS 版本具有相同行为。

## 2. 修复方案与不变量

### 路由和 DNS

1. `allowLocalLan` 独立进入完整 handoff 和脱敏 Want 配置，扩展恢复不再依赖派生的
   `allowBypass`。服务端实际 CSTP 配置仍是地址、DNS、MTU 和路由的来源。
2. 在扩展获得实际网络配置后，将包含与排除策略转换为正向 CIDR，再交给 ArkTS 适配。
   IPv4 和 IPv6 都按最长前缀匹配：更具体的包含路由优先；同前缀冲突按排除处理。
3. 私有 DNS 的 `/32` 或 `/128` 可以覆盖较宽的 LAN 排除。例如默认路由、
   `10.0.0.0/8` 排除和 `10.10.10.1/32` DNS 同时存在时，DNS 仍进入隧道，
   其他被排除地址走上行网络。路由优先级由前缀长度决定，不能依赖数组插入顺序。
4. 嵌套和重复排除与输入顺序无关；输出为空时拒绝创建 VPN，防止系统补出默认路由。
5. 不增加公共 DNS、虚构隧道地址或网关绕行地址。LAN 偏好的网段范围沿用现有策略，
   本次不把它扩展为新的自动网段发现功能。
6. 关闭本地 LAN 访问偏好不会删除服务端显式推送的排除策略。纯全路由验收必须使用
   不含服务端排除的配置；`forceGlobal` 选择默认路由时也不能假定服务端的普通
   split-include 主机路由仍保留，DNS 主机路由则由专门路径补充。

### 生命周期、统计和清理

1. 每次准备、挂接与会话退出都关联当前 attempt 和 generation；异步操作返回后重新
   核对归属，防止旧操作覆盖新连接。主循环启动后立即退出也必须保持终态。
2. 原生主循环退出后发布真实终态及错误；正常取消不覆盖已经生效的用户断开或失败结果。
3. 扩展定时驱动健康检查和真实流量统计，并在原生会话失效后销毁平台 VPN。
   UI 终态也必须传播到扩展，避免只修改页面状态而留下隧道。
4. UI 独立记录远端 revision 的最后推进时间。更新停滞达到 15 秒后，再给 6 秒观察
   宽限；该宽限从 UI 实际观察到停滞时开始，为休眠恢复后的扩展留出刷新机会。
   不按 `sync` 调用次数计时，不用 UI 自己的写帧表示扩展存活。
5. 手动断开和失败状态不能被迟到 running 帧恢复；新请求等待旧连接清理完成。
   同一次尝试的 IPC 重绑与新连接请求须区分处理。
6. 真正 dry-run 才能使用模拟流量。生产 UI 使用扩展发布的真实计数，并在状态检查完成
   后判断自动重连边沿；自动重连仍遵循配置且限于应用现有活动状态行为。
7. 原生等待退出有明确上限。`anyconnect 0.1.1` 的 `setup_tun_fd_borrowed` 会复制
   平台描述符；超时后丢弃线程句柄不会强行终止 C 线程，因此仍须隔离旧会话并完成
   平台清理，不能把“join 返回”直接当成“线程已退出”。
8. ArkTS 的 Promise 超时不代表底层操作取消。创建、进程保护、挂接和销毁保留原始
   操作的完成屏障，新连接必须等待旧操作及销毁完成。销毁报错时不越过屏障继续建隧道；
   这是安全优先的取舍，不承诺平台操作永久挂起时仍能在原扩展进程内恢复。
9. 扩展只在已校验的新 Want 绑定点接管 attempt，普通状态同步不改变所有者。
   迟到 Want 在递增请求代次、销毁或 IPC 替换之前拒绝。停止操作在同一核心锁内验证
   归属并取出旧 pending/running，锁外取消和等待，完成后再次核对 generation/attempt；
   禁止旧 stop 通过全局 running=false 取消新请求。

### 套接字保护

每次注册捕获本次平台连接和回调，不在调用时转用下一次连接的注册项。保护错误按注册
归属存储和处理，旧会话迟到的错误不能覆盖或清除新会话错误。保护注册失败应中止
连接流程；保护调用错误须进入启动或运行期失败处理，而不止写一条日志。

## 3. 与附件补丁的主要差异

- 不采用“删除所有排除路由”的建议，保留用户及服务端的路由策略。
- 不把原有 DNS 去重描述为新修复；增加集中处理与可验证的语义测试。
- 不采用按调用次数累加的 watchdog；使用远端 revision 与本地单调时间。
- 补充附件没有完整解决的 UI 重连判断顺序、生产模拟流量、异步销毁与新请求竞态。
- 不整体采纳附件中的 Windows 构建脚本、机器特有签名配置和格式化改动；Rust 代码
  仍按项目格式化工具整理。
- 不将附件中“真机验证通过”的声明作为本分支的验证结果。

## 4. 验证环境

验证时本地最新设备矩阵目录名（下文用 `QEMU_MATRIX_DIR` 指向其安装位置）：

```text
device-matrix-20260905-m144-runtime-fixes
```

矩阵清单更新时间为 2026-09-07 13:17（Asia/Shanghai）。这与每个镜像的实际构建时间不同：

| 镜像 | 实际构建时间 | 矩阵记录的压缩包 SHA-256 |
| --- | --- | --- |
| `openharmony-qemu-arm64-arm64_virt-phone` | 2026-09-05 23:12 | `8a6041df2036ee4a5ddefb2de5f489265bcac1ab1b913d38ce3f1d3257c27b30` |
| `openharmony-qemu-arm64-arm64_virt-2in1` | 2026-09-07 00:54 | `4d9e9f25029233399e687f5e0e70318a824744ae84820318cafabb07f96ed9c5` |

两个压缩包均已独立计算 SHA-256，与清单一致。两个镜像均声明 `standard_vpn=true`，
默认分辨率 800×500。测试须使用临时副本，保留原始镜像。

QEMU 签名使用本机已有的开发测试工具：

```text
tool: hapsigner-rs, target/release/hap-sign
version: 0.1.0
SHA-256: 36d61b5bd7cf7a38b1281b82ac7cdd63bea37cb36fd4a3c28f6e2328fb81338e
```

该工具使用公开的 OpenHarmony 开发测试密钥，仅用于 QEMU 验证。未使用或替换发布签名。
自动测试使用本地 ocserv 的 `demo` 测试账号、自签名证书和仅 CSTP 的连接配置；
测试配置关闭严格证书校验，但没有更改应用新配置的严格证书校验默认值。

本轮重新构建并签名的测试包（主线程已独立核对 SHA-256）：

```text
entry/build/default/outputs/default/entry-default-signed.hap
3df393efe09be3a805a17a00ed03112b31ea3bd7bcd7eb4b234ec9ecf55edac2
```

## 5. 验证记录

最终测试包在本地最新构建的 ARM64 2-in-1 镜像上完成本文的 IPv4 路由与生命周期
矩阵，脚本退出码为 0。验收范围及未覆盖项见下文；不据此宣称全部真机场景通过。

最新 ARM64 2-in-1 的复现命令（已将实际执行时的机器路径参数化，从仓库根目录运行）：
先设置 `QEMU_MATRIX_DIR` 为上述设备矩阵目录、`OHOS_NDK_HOME` 为 OpenHarmony SDK
目录、`LAN_ENDPOINT_HOST` 为宿主机可达的 LAN 地址。本轮 LAN 地址为 `192.168.3.28`。

```sh
: "${QEMU_MATRIX_DIR:?请先设置设备矩阵安装目录}"
: "${OHOS_NDK_HOME:?请先设置 OpenHarmony SDK 目录}"
: "${LAN_ENDPOINT_HOST:?请先设置宿主 LAN 地址}"
QEMU_PACKAGE_DIR="$QEMU_MATRIX_DIR/openharmony-qemu-arm64-arm64_virt-2in1" \
HAP_PATH="$PWD/entry/build/default/outputs/default/entry-default-signed.hap" \
OHOS_CLANG="$OHOS_NDK_HOME/native/llvm/bin/aarch64-unknown-linux-ohos-clang" \
QEMU_HDC_HOST_PORT=5578 HDC_TARGET=127.0.0.1:5578 \
RUN_ISSUE3_MATRIX=1 RUN_DEADLINE_TEST=0 LAN_ENDPOINT_HOST="$LAN_ENDPOINT_HOST" \
ARTIFACT_DIR="$PWD/smoke-logs/issue3-2in1-matrix-fix3" \
QEMU_TEST_TMP_PARENT="$PWD/smoke-logs/qemu-tmp" \
./scripts/e2e-qemu-arm64.sh
```

临时副本采用 PSSD 同卷 CoW 克隆，原镜像不变。测试专用容器、监听进程与 QEMU
由脚本按本次实例收尾；日志保留在 artifact 目录。探测器仅打印真实 UID、源地址及
`SO_MARK`，不强制绑定网络、不修改 socket 标记。

| 检查 | 当前结果 |
| --- | --- |
| 独立路由语义测试 | 已通过 3 项测试；IPv4、IPv6 各 128 组策略，每组 256 地址，正反排除顺序合计 131,072 次逐地址比较，另有嵌套排除与 DNS 反例 |
| 真实 OpenConnect 后端回归 | `cargo test -p hopenconnect_core --features native-anyconnect --all-targets`：90 项核心单测 + 3 项路由测试 + 3 项 IPC 契约测试通过；另 1 项需 headend 的 live 测试被忽略，不能计为通过 |
| 默认 feature 回归 | `cargo test -p hopenconnect_core --all-targets`：71 项核心单测 + 3 项路由测试 + 3 项 IPC 契约测试通过；与 native-feature 测试有重叠，不累加为独立用例数 |
| UI 集成测试 | `cargo test -p hopenconnect_ui --tests`：18/18 通过（日志筛选 5、日志 UI 3、启动 UI 4、VPN 启动 6）；其中 ArkTS 契约是源码约束检查，不是 Promise 时序的运行模拟 |
| 补丁格式与脚本语法 | `git diff --check`、两个 shell 脚本的 `bash -n`、探测器在宿主 clang 和 OHOS aarch64 clang 下的 `-Wall -Wextra -Werror -fsyntax-only` 均通过。提交整理移除了无关格式变更；全仓库 `cargo fmt --check` 的基线差异位于 UI 的 `bridge/mod.rs`、`view/mod.rs` 和 `logs_ui_contract.rs`，未混入本次修复 |
| 正式功能 HAP 构建、QEMU 测试签名与安装 | `SIGN_HAP=0 ./scripts/package-hap.sh` 构建 native/ArkTS HAP 成功，再以本地公开开发测试密钥签名；最终包在最新 2-in-1 安装和运行成功 |
| split / full 路由及 LAN 开关 | 最终包 `issue3-2in1-matrix-fix3` 全部通过 |
| 应用 UID 的 DNS、TCP 及 headend 流量证据 | fix3 的 UID `20010041` 探测通过：公网/内网经 TUN，LAN-on 经上行；用源地址与专用转发计数共同确认，而非 root shell 连通性 |
| 服务端踢线 / 原生主循环退出 | fix3 全路由 LAN-off：真实 native `status -32` 上报并清理，新服务端会话约 1 秒出现；UI 恢复 Connected，内网与 LAN TCP 探测通过且确认经 TUN |
| 扩展进程退出 / 心跳停止 | fix3 全路由 LAN-off：`kill -9` 扩展，约 22 秒后新扩展、TUN 与 UI Connected 成立；日志明确记录心跳停止，恢复后内网/LAN TCP 和隧道路径证据通过 |
| 手动断开 / 新尝试重连 / 旧状态不复活 | fix3 手动断开后 UI 未连接且 TUN 消失，再连接生成新 attempt 并恢复 Connected/TUN；旧帧不复活另有核心回归测试 |
| 最新 ARM64 2-in-1 完整矩阵 | `ARM64 QEMU AnyConnect E2E OK`，退出码 0；原镜像不变，本次 QEMU、测试监听及容器已收尾 |

QEMU 的 IPv4 数据路径验收、IPv6 路由算法单测、真机休眠及网络切换应分别说明，
不能相互替代。完整日志保留在 `smoke-logs/issue3-2in1-matrix-fix3`。

LAN 开关的验收应检查路径而不是简单检查可达性：关闭绕行时，该目标的流量应进入
VPN；开启绕行时则走上行。测试 headend 与宿主在同一网络，关闭绕行后仍可访问宿主
不一定是错误。应结合探测器实际源地址、只读 `SO_MARK` 和 headend 上针对目标的
专用转发计数判断；空 FORWARD 链的 policy 计数不能单独作为旁路证据。

### 首次设备测试发现的验收条件问题

`smoke-logs/issue3-phone-baseline-5568` 已记录真实会话建立和应用 UID `20010041`
访问 `example.com:443` 成功，但 `10.10.10.1:443` 失败，故这次整体验收不算通过。
源码核对发现，`scripts/dev-ocserv.sh` 默认既推送 `route = default`，也推送
`no-route = 10.0.0.0/8` 等三个私网排除。保留这些排除策略时，`10.10.10.1` 本来
就不应走 VPN；旧 E2E 对该地址必须成功的断言与服务器配置冲突。

因此，纯全路由测试应显式设置 `OCSERV_DISABLE_NO_ROUTES=1`，而服务端排除测试
须独立断言绕行行为；不能为了让旧测试变绿而删除生产排除策略。首轮设备日志实际
记录了 33 条正向路由、`protect(fd)` 成功和 native 挂接完成。

后续 `smoke-logs/issue3-phone-matrix-final` 在纯全路由下再次建立连接，但受控
`10.10.10.1:18443` 端点不可达。测试设施随后显式配置端点地址与监听，并保存
监听日志。`smoke-logs/issue3-phone-matrix-final2` 中，该端点与公网 DNS/TCP
均成功；LAN-off 的宿主端点也可达。因为当时脚本将“LAN-off 可达”直接当作失败，
且公网证据只有空 FORWARD 链计数，这轮不能作为完整路由矩阵通过的证据，也不能
仅凭这些结果断言应用错误地绕过 VPN。最新矩阵须使用前述路径证据重新判定。

### 首轮最新 2-in-1：发现并补修自动重连准备态泄漏

`smoke-logs/issue3-2in1-matrix-final` 使用上一版测试包
`996c1cc36a08ba2f1c26deee8975cdbd4baddb337e557ffec25dde7d8b0da4b1`，
完成了授权取消后的 120 秒启动总期限，以及以下真实应用 UID 路径检查：

- 全路由、LAN 绕行关闭：公网 TCP 与宿主 LAN TCP 的源地址均为 `10.10.10.231`；
  headend 对应专用转发计数均从 0 增至 3。
- 全路由、LAN 绕行开启：公网仍经 `10.10.10.231`；LAN 源地址改为 QEMU 上行
  `10.0.2.15`，headend LAN 计数保持 3。
- 分流：受控 VPN 内网 `10.10.10.1:18443` 经 `10.10.10.231`，宿主 LAN 经
  `10.0.2.15`，LAN 计数仍保持 3。

但服务端执行踢线后，该轮没有自动恢复，UI 显示真实的 mainloop `status -32`
终态。继续分析定位到 UI 的 `pending_native` 在认证交接完成后仍被保留，导致下一次
自动准备被“原生会话已存在”的保护条件拒绝。手动断开会清除此字段，所以先前手动
重连测试不能暴露这个缺陷。修复限定在 UI lane、匹配 attempt 且收到扩展接管或终态
确认之后释放准备态，并增加 native-feature 回归。上表的最终测试包包含该补修；
此轮失败不能视为自动恢复验收通过，也没有执行到后续扩展强杀步骤。

### 第二轮最新 2-in-1：首次接管中性帧的时序回归

`smoke-logs/issue3-2in1-matrix-fix1` 使用测试包
`1f5f92ff46fd22ca4b418c1f1275e9351dd47cba2141811cf80347f35ba7cd51`，
在等待首次 UI Connected 时失败。设备日志显示扩展已经 `startVpn` 成功、TUN 为
UP/RUNNING，ocserv 会话也持续在线，但 UI 显示“未连接”，因此不能记为通过。

根因是首次附加 IPC 时，扩展可能发布“已接管但尚未启动”的 Pending 中性帧。
UI 将它误判为断开，之后又正确地拒绝从 Disconnected 接受迟到的 Connected，
导致页面无法进入在线状态。补修需同时满足“中性帧不取消启动”和“用户取消后不复活”，
不能简单重新允许所有 Disconnected→Connected 转换。新增顺序回归覆盖该区别。

### 第三轮最新 2-in-1：旧 stop 误取消自动重连

`smoke-logs/issue3-2in1-matrix-fix2` 使用测试包
`028df761a48ab66812cf103f40c9937025bd279105d920f3c0762ee9e33997dc`。
首次启动、全路由 LAN-off / LAN-on、分流以及应用 UID 的内外网探测全部通过。
LAN-off 流量的隧道源地址为 `10.10.10.231`，专用 headend 计数 0→3；
LAN-on 为上行源地址 `10.0.2.15`，计数保持 3。

踢线后，这次已能触发新的认证与平台 start attempt，但新尝试被旧清理取消：

- 22:43:54.259：扩展开始清理旧 native 终态。
- 22:43:54.287：UI 发布新的 start attempt。
- 22:43:54.462：新 start 报 `VPN extension start was cancelled`。

源码对应 `stop_vpn()` 先等待 `disconnect()`（包括 200 毫秒等待），再全局调用
`set_platform_vpn_running(false)`；后者的同步可能提前采纳 UI 的新 attempt，随后
把新的 Pending 取消。这个结果进一步要求：普通同步不得擅自改变扩展的会话所有者，
旧清理完成必须按原 attempt/generation 应用。该轮整体仍未通过，也未执行扩展强杀。

### 最终最新 2-in-1：路由、踢线、扩展强杀与手动重连通过

`smoke-logs/issue3-2in1-matrix-fix3` 使用上文 SHA-256 为 `3df393ef…55edac2`
的最终测试包，2026-09-07 23:17–23:18（Asia/Shanghai）完成矩阵。
由指定 sol / xhigh 代理实现和编制测试；其最终启动调用停在审批层后，由具备执行权限的
主线程运行同一测试命令，代理继续独立复核产物。没有修改原始 OHOS 镜像或系统源码。

- 全路由 LAN-off：公网 TCP 和受控 LAN TCP 源地址为 `10.10.10.231`，
  对应 headend 专用计数分别 0→3。
- 全路由 LAN-on：LAN 源地址为 `10.0.2.15`，LAN 计数 3→3；公网仍经
  `10.10.10.231`。分流下，受控内网走隧道，LAN 走上行。
- 切回全路由 LAN-off 后执行 `occtl disconnect user demo`：服务端会话
  `163→214`，观察到新会话耗时约 1 秒。23:18:12.507 扩展发现 native 终态并清理，
  23:18:12.903 新 attempt `1788794292787903522-2` 完成。恢复后内网与 LAN
  探测成功，LAN headend 计数 6→9，源地址仍为隧道地址。
- 对扩展 PID `3432` 执行 `kill -9`：约 22 秒后新 PID `3562`、TUN 和 UI
  Connected 成立。23:18:37 日志记录 `VPN extension heartbeat stopped`，
  新 attempt `1788794317351972617-3` 于 23:18:37.774 完成。恢复后两类
  TCP 探测成功，LAN 计数 9→12，源地址仍为隧道地址。
- 最后手动断开，23:18:40.921 扩展 `onDestroy`；脚本确认 UI 未连接、TUN 消失后
  再连接。23:18:43.129 新 PID `4076`、attempt `1788794322734626245-4`
  完成，最终布局已连接，TUN UP/RUNNING、RX 7 / TX 5。

1 秒是新服务端会话的轮询观测时间，22 秒是新扩展、TUN、UI Connected 的联合观测
时间，并非分别精确测量的故障检测延迟；数据探测紧随恢复判定执行。这些是本地受控
环境的一次测量，不是恢复时延保证。探测 UID/euid/gid 均为 `20010041`，SO_MARK 为 0。

主要证据文件：`app-runtime.log`、`hilog-entry.log`、`server-kick-recovery.txt`、
`extension-kill-recovery.txt`、`probe-*.txt`、`forward-*.txt`、`tun-reconnected.txt`。
本次临时 VM、headend 容器、LAN 监听和临时镜像副本已清理，日志保留；没有停止原有
占用 5558 端口的其他 QEMU 实例。

### 尚未覆盖及后续验收

- Mate 80 / 商用旧版 HarmonyOS、真机锁屏休眠和 Wi-Fi/蜂窝切换。
- IPv6 真实数据通路、DTLS；当前 IPv6 证据仅为路由语义测试，QEMU headend 使用 CSTP。
- UI 统计页的视觉采样与数值对照；本轮确认真实探测/TUN 计数，统计链修复不等于已做
  UI 数值验收。ArkTS LocalUnit 新用例也未单独运行 Hypium。
- 长时间物理断网而 OpenConnect 仍处于内部重连的状态；精确 Reconnecting 信号属于
  [后续架构设计](issue-3-platform-design.md)，不能用扩展心跳替代。
- 平台 protect/create/destroy Promise 失败及永久挂起的设备故障注入；当前覆盖源码
  契约和核心回归，不承诺所有平台异常都已实测。
- 授权取消后的 120 秒总期限在前一测试包 `issue3-2in1-matrix-final` 通过；最终 fix3
  为 `RUN_DEADLINE_TEST=0`，不把旧包的该项结果写成最终包重新执行。

变更按应用修复、QEMU 回归脚本、文档三个主题整理提交在
`fix/issue-3-vpn-routing-lifecycle` 分支；未推送、创建 PR 或关闭 issue。
应用修复提交为 `fc41d72`，QEMU 回归脚本提交为 `9234175`。本次提交整理仅参数化
文档路径、补充核对结论并移除无关格式差异，未新增功能修改；重新运行的核心/native
与 UI 回归通过，QEMU 结论沿用前述最终功能包的实测记录，没有冒充本次重新运行。
两个 `entry/oh_modules/@ohos-rs` 生成链接仅取消 Git 跟踪，磁盘依赖仍保留。

## 6. 对照 issue 与附件的最终核对

重新核对 [issue #3 正文](https://github.com/harmony-contrib/h-openconnect/issues/3)
及其附件报告；核对时没有新增评论。以下编号沿用附件，不把附件的推测或作者的真机
结论直接视为本分支证据。

| 编号 / 项目 | 代码结论 | 已有证据与限制 |
| --- | --- | --- |
| 1.1 排除路由黑洞 | 已采用正向 CIDR 兼容路径，保留排除策略；本地 classic OHOS 源码支持排除字段，因此不采纳“所有 OHOS 都不支持”的归因 | 路由 oracle 与 QEMU full/split 数据路径通过；未覆盖作者的商用固件 |
| 1.2 LAN 偏好 handoff 丢失 | 已独立传递 `allowLocalLan`，兼容旧 handoff | 核心回归及 QEMU 全路由 LAN-off/on 通过 |
| 1.3 DNS 路由缺失/重复 | 基线已有去重，未证实“多次恢复必然膨胀”；本次集中规范化并加固去重和最长前缀规则 | DNS/嵌套路由回归及 QEMU 应用 UID DNS/TCP 通过；IPv6 仅算法验证 |
| 1.4 protect 注册静默、持锁阻塞 | 已改为注册失败中止、锁外调用、按注册与会话隔离错误 | 并发回调回归通过；设备 Promise 失败/永久挂起未注入 |
| 2.1 主循环退出自报及 generation 守卫 | 已实现，旧 worker 与 attach 即退不能覆盖新会话 | 核心回归及 QEMU 真实 `status -32` 踢线自动恢复通过 |
| 2.2 有界 join | 已实现有界等待，超时隔离旧 worker | 已核对实现；未注入 C worker 永久卡死，detach 不等于杀死线程 |
| 2.3 扩展心跳、真实统计、僵尸 TUN 清理 | 已实现扩展 tick、生产真实计数及终态清理 | QEMU 故障恢复和 TUN 计数通过；未对 UI 统计页做数值/视觉验收 |
| 2.4 UI 看门狗 | 已实现单调时间停滞检测与唤醒宽限，不采用按 tick 次数累计 | 核心时钟回归通过；QEMU 强杀后约 22 秒恢复；未覆盖真机锁屏唤醒 |
| 2.5 扩展侧终态对账 | 已实现匹配 attempt 的终态传播、取消和清理 | 核心终态回归、QEMU 手动断开通过；最终包未重新执行 120 秒授权期限场景 |
| 3.1 旧 running 帧复活 UI | 已限制 Connected/Starting 晋级并保护终态 | 迟到帧回归及 QEMU 手动断开、新 attempt 重连通过 |
| 3.2 rebind 复活死 TUN | 已先检查健康状态，死亡会话清理后重建，旧 Want 在修改资源前拒绝 | 核心归属/启动契约通过；未单独注入每个毫秒级 rebind 窗口 |
| 3.3 订阅取消时热自旋 | 已加入 false 返回后的 250 毫秒退避并停止旧订阅 | 源码检查；未单独量测异常订阅下的设备 CPU |
| 3.4 NAPI 声明未同步 | `extensionTick` 及新增归属接口已同步声明 | Rust、ArkTS HAP 构建通过 |
| 3.5 生成依赖链接入库 | 已忽略生成目录并取消两个 symlink 的 Git 跟踪 | 磁盘链接保留，不删除已安装依赖 |

**结论：已确认的核心缺陷已修复，最终 QEMU 矩阵通过，但不能宣称 issue 描述的全部
情境都已完整解决，也不建议仅凭本轮结果关闭 issue。**

区别在于：主循环已经退出和扩展已经死亡的假在线已闭环；但网络切换、DPD/物理断网
期间 OpenConnect 仍在内部重连时，worker 与心跳可能仍存活。本分支尚未提供精确的
`Reconnecting` 状态，因此这不是单纯“没测过”，还有明确的后续能力需要实现。
同样，平台 Promise 永久不返回时，当前清理屏障会阻止新建连接，但没有完整的自动
恢复方案；`withTimeout` 没有被删除，也不能把等待超时当作底层取消或资源释放。

达到完整关闭条件，还需补齐内部重连状态与恢复设计，完成商用真机网络切换/休眠、
UI 统计、DPD/长时间断网，以及平台操作异常的对应验收。附件中的 Windows 构建脚本
属于可选工具移植，不是上述缺陷的必要修复，本次未引入。
