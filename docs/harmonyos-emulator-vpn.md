# HarmonyOS 模拟器 VPN 接入标准

本文定义 H-OpenConnect 在 DevEco HarmonyOS 模拟器上启动真实
`VpnExtensionAbility`、创建系统 TUN 并同步应用状态的标准实现和验收方法。该方案不
修改只读 `system` 分区、不复制系统私有 `.so`、不 mock VPN，也不以单元测试代替真实
隧道；但它会为 debug HAP 更新模拟器 `userdata` 中的 VPN 授权状态，不能描述为“完全
不修改模拟器配置”。

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

### 当前验证环境并非原始 userdata

最终跑通真实隧道时，模拟器 SettingsData 中已经存在以下授权记录：

```text
com.richerfu.h_openconnect=1
com.richerfu.h_openconnect_100=1
```

这两条记录位于模拟器 `userdata`，表示当前 bundle 及 user 100 已获得 VPN 授权。调试
期间曾通过离线更新 SettingsData 的方式写入；当前 App 启动时调用的
`updateVpnAuthorizedState()` 意图更新同一类授权状态，但尚未在空记录上验证首次建档。
因此，本次结果证明的是“授权状态已经写入后，真实 VPN Extension/TUN 链路可以工作”，
不能据此宣称未经初始化的全新模拟器快照也能直接启动 VPN。

如果需要证明 App 能独立完成初始化，必须在恢复出厂或新建的模拟器实例上重新安装
debug HAP，并确认首次启动就出现系统 `UpdateVpnAuthorize result. ret = 0`，随后真实
连接成功。现有跑通记录不能代替这项冷启动验证。

### 模拟器数据修改账本

当前排查和最终验证涉及的模拟器侧数据如下。后续判断是否为“纯净镜像”必须同时检查
`system`、`sys_prod` 和 `userdata`，不能只检查 `system.img`。

| 数据 | 实际修改 | 当前是否依赖 | 结论 |
| --- | --- | --- | --- |
| `userdata` SettingsData | 在主库和 slave 库的 `SETTINGSDATA` 表写入 bundle 授权记录 | 是 | 当前已验证方案的授权前置条件 |
| App 启动授权 | debug 启动时调用 `updateVpnAuthorizedState(bundleName)` | 已执行，但首次建档能力未单独验证 | 作为自动初始化候选方案保留 |
| `system` VPN 白名单 | 修改 `allow_connect_vpn.json` | 否 | 会触发镜像校验问题，已撤销 |
| `system` 参数文件 | 排查期间实验过 `ollie.para` | 否 | 不进入标准方案 |
| HAP 私有系统库 | 曾尝试复制/链接 VPN 系统 `.so` | 否 | 命名空间、依赖和跨进程状态均不成立 |

SettingsData 位于 `userdata` 文件系统中的：

```text
/app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db
/app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata_slave.db
```

两份数据库的 `SETTINGSDATA` 表均写入：

```sql
INSERT INTO SETTINGSDATA(KEYWORD, VALUE)
VALUES ('com.richerfu.h_openconnect', '1')
ON CONFLICT(KEYWORD) DO UPDATE SET VALUE = excluded.VALUE;

INSERT INTO SETTINGSDATA(KEYWORD, VALUE)
VALUES ('com.richerfu.h_openconnect_100', '1')
ON CONFLICT(KEYWORD) DO UPDATE SET VALUE = excluded.VALUE;
```

第一条是 bundle 授权，第二条是 bundle 与 user 100 的授权。不能把 `100` 固定套用到
其他用户实例；必须以目标模拟器实际 user ID 为准。

### `system` 分区白名单不是当前生效项

排查期间曾修改：

```text
/system/etc/communication/netmanager_enhanced/vpn/allow_connect_vpn.json
```

实验内容包括把 `com.richerfu.h_openconnect` 加入 `allowConnectVpnBundleName` 或
`allowVpnStartWithoutCheckPermissions`。该修改会触发镜像文件校验问题，随后已从当前
启动链路撤销。当前运行实例的 `system.img.qcow2` 没有已分配的数据区块，仍直接读取
DevEco 原始 `system.img`；所以最终成功不能归因于这份白名单补丁。

直接修改只读系统镜像中的白名单、系统配置或系统应用会破坏镜像文件校验。复制系统
内部依赖库到 HAP 还会引入命名空间和 ABI 问题，例如 `libzuri.z.so`、
`libnet_data_share.z.so` 或 `libc++.so` 的级联加载失败；即使加载成功，也不能因此获得
系统服务进程中的同一份状态。

这些 `system` 分区修改方式不属于本项目的标准方案；`userdata` 授权初始化则是当前
模拟器兼容流程的一部分。

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

模拟器必须先具备上述持久化授权状态，再执行无 FD 的 VPN Extension 启动。授权有两种
实现方案；二者解决的只是授权，不替代后续 Extension 启动、FD handoff、TUN 创建和
OpenConnect 数据面。

### 方案 A：debug App 调用系统授权接口

系统授权弹窗缺失时，debug HAP 在插件安装阶段调用：

```text
updateVpnAuthorizedState(<current bundle name>)
```

该接口的目标是更新当前 bundle 的 VPN 授权状态，不是单纯的进程内开关。当前实例中
可以确认调用返回 `true`，系统日志为 `UpdateVpnAuthorize result. ret = 0`；但调用发生
前 SettingsData 记录已经由方案 B 写入，因此尚未证明它能在全新 `userdata` 上首次创建
授权记录。

实现必须同时满足：

- 仅在 `BuildProfile.DEBUG` 为真时执行；
- bundle name 来自当前构建配置，不能硬编码其他应用；
- release HAP 不调用隐藏 API，真机继续由系统授权 UI 管理；
- 日志中同时核对应用返回值和系统 `UpdateVpnAuthorize result. ret = 0`；
- 应用启动时只恢复本应用留下的精确 stale attempt，不清理其他应用或新 attempt。

相关代码集中在
`entry/src/main/ets/vpnability/VpnEmulatorCompatibility.ets`，隐藏 API 的本地声明位于
`entry/src/main/ets/types/vpnExtensionDebug.d.ts`。不得把这段逻辑扩展到 release。

方案 A 的验收必须从全新的模拟器实例开始：安装前确认两条 bundle 记录不存在，首次
启动 App 后确认记录出现，再完成一次真实隧道连接。在完成这项验证前，不能把方案 A
标记为可替代方案 B 的独立初始化方案。

### 方案 B：离线预置 userdata 授权

这是当前真实连接已经验证过的模拟器初始化方式，适合固定的开发或 CI 模拟器基线。
它修改的是实例自己的 `userdata.img.qcow2`，不修改 SDK 中的基础镜像。

操作顺序：

1. 创建并启动一次模拟器，安装目标 debug HAP，确保 SettingsData 数据库已经生成。
2. 正常关闭模拟器；禁止在 QEMU 仍持有镜像时修改 `userdata.img.qcow2`。
3. 备份整个实例的 `userdata.img.qcow2`，备份名称应包含修改前时间。
4. 使用 `qemu-img convert -O raw` 合并 qcow2 与 backing file，得到临时 raw 镜像。
5. 使用 `debugfs` 从上述 `rdb` 目录导出 `settingsdata.db` 和
   `settingsdata_slave.db`，并保存原文件的 uid、gid、mode、ACL、`user.security` 和
   `security.selinux` 扩展属性。
6. 对两份数据库执行上面的 SQL，并分别执行 `PRAGMA integrity_check`。
7. 清除镜像内旧的 `-wal`、`-shm` 和 `-dwr` 文件，把两份数据库写回原路径，恢复原始
   文件属性。当前验证镜像中的属性为 mode `0660`、uid/gid `20003`、
   `user.security=s1`、`security.selinux=u:object_r:appdat:s0`；其他版本必须以原文件为准。
8. 把修改后的 raw 镜像转换回 qcow2，保留原 `userdata.img` backing file，然后运行
   `qemu-img check`。
9. 替换实例镜像后冷启动，查询两份数据库并检查真实 VPN 日志与 `vpn-tun`。

导出数据库和原始扩展属性的命令形式如下。`debugfs` 在 Homebrew e2fsprogs 中通常不在
默认 `PATH`，应显式设置其路径：

```bash
DEBUGFS="$(brew --prefix e2fsprogs)/sbin/debugfs"
RDB_PATH=/app/el1/0/database/com.ohos.settingsdata/entry/rdb

"$DEBUGFS" -R \
  "dump $RDB_PATH/settingsdata.db $WORK_DIR/settingsdata.db" \
  "$WORK_DIR/userdata.raw"
"$DEBUGFS" -R \
  "dump $RDB_PATH/settingsdata_slave.db $WORK_DIR/settingsdata_slave.db" \
  "$WORK_DIR/userdata.raw"

"$DEBUGFS" -R \
  "ea_get -f $WORK_DIR/main.acl $RDB_PATH/settingsdata.db system.posix_acl_access" \
  "$WORK_DIR/userdata.raw"
"$DEBUGFS" -R \
  "ea_get -f $WORK_DIR/main.user_security $RDB_PATH/settingsdata.db user.security" \
  "$WORK_DIR/userdata.raw"
"$DEBUGFS" -R \
  "ea_get -f $WORK_DIR/main.selinux $RDB_PATH/settingsdata.db security.selinux" \
  "$WORK_DIR/userdata.raw"
```

对 slave 文件重复保存 `system.posix_acl_access`、`user.security` 和
`security.selinux`。写回时使用 `debugfs -w -f <command-file>`；命令文件必须先删除对应
数据库的 `-wal`、`-shm`、`-dwr` 和旧数据库，再依次执行 `write`、`sif` 与
`ea_set -f`。实际验证使用的主库写回结构如下，slave 使用其自己的文件和属性副本：

```text
rm /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db-wal
rm /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db-shm
rm /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db-dwr
rm /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db
write <WORK_DIR>/settingsdata.db /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db
sif /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db mode 0100660
sif /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db uid 20003
sif /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db gid 20003
ea_set -f <WORK_DIR>/main.acl /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db system.posix_acl_access
ea_set -f <WORK_DIR>/main.user_security /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db user.security
ea_set -f <WORK_DIR>/main.selinux /app/el1/0/database/com.ohos.settingsdata/entry/rdb/settingsdata.db security.selinux
```

`<WORK_DIR>` 必须在生成命令文件时替换为绝对路径；`debugfs` 不展开 shell 变量。uid、
gid 和 mode 也必须来自修改前的 `debugfs stat`，上面的数值只代表本次验证镜像。

主库和 slave 库必须保持一致，示例核对 SQL 为：

```sql
SELECT KEYWORD, VALUE
FROM SETTINGSDATA
WHERE KEYWORD IN (
  'com.richerfu.h_openconnect',
  'com.richerfu.h_openconnect_100'
);
```

预期两份数据库都返回两行且值均为 `1`。只修改主库、遗留 WAL，或者写回时丢失 ACL、
SELinux 标签，都可能导致 SettingsData 恢复旧值、拒绝访问或在冷启动后重建数据库。

建议使用以下目录变量组织离线操作，避免误改 SDK 基础镜像：

```bash
EMU_DIR="<DevEco 模拟器实例目录>"
USERDATA_QCOW="$EMU_DIR/userdata.img.qcow2"
WORK_DIR="$(mktemp -d /private/tmp/hvpn-userdata.XXXXXX)"

cp "$USERDATA_QCOW" \
  "$EMU_DIR/userdata.img.qcow2.before-vpn-auth-$(date +%Y%m%d%H%M%S)"
qemu-img convert -O raw "$USERDATA_QCOW" "$WORK_DIR/userdata.raw"
```

数据库写回和 qcow2 替换必须在模拟器完全停止后执行。`BASE_USERDATA` 应取
`qemu-img info --backing-chain "$USERDATA_QCOW"` 显示的原 backing file：

```bash
qemu-img convert -f raw -O qcow2 \
  -B "$BASE_USERDATA" -F raw \
  "$WORK_DIR/userdata.raw" "$WORK_DIR/userdata.patched.qcow2"
qemu-img check "$WORK_DIR/userdata.patched.qcow2"
```

检查成功后才能用 `userdata.patched.qcow2` 替换实例文件。不得把 SDK 目录中的基础
`userdata.img` 作为写入目标，也不得在未验证备份可用前删除原实例文件。

### 方案选择

- 当前确定可工作的基线：方案 B 预置授权 + 标准无 FD 启动与 FD handoff。
- 目标方案：方案 A 在 debug App 中自动授权；需要补做全新 `userdata` 的首次启动验证。
- release HAP 和真机：均不使用 A/B，由系统授权 UI 管理。
- 无论选择 A 还是 B，都不能恢复已撤销的 `system` 白名单补丁或复制系统私有库。

### 回滚 userdata 修改

先关闭模拟器，再恢复修改前备份的 `userdata.img.qcow2`；或者删除该模拟器实例并重新
创建。恢复整个 userdata 会同时回退已安装应用、应用数据和系统用户设置，因此不要只
凭文件名覆盖，必须核对实例目录和备份时间。回滚后用全新安装流程重新验证。

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
