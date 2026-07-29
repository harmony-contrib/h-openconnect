# H-AnyConnect E2E

## Architecture (full anyconnect-rs)

```
UI (arkit) ──► hanyconnect_core::SessionEngine
                     │
                     │ prepare_connect (dry_run=false)
                     ▼
              anyconnect-rs Client
              · obtain_cookie
              · 保存 webvpn cookie 到跨进程 handoff
                     │
                     ▼
              EntryAbility.requestStartVpn(optionsJson)
                     │
                     ▼
              HAnyConnectVpnExtensionAbility
              · 读取 handoff，使用标准 webvpn cookie 恢复会话
              · make_cstp_connection
              · setup_dtls（失败时按协议回退 CSTP）
              · network_config → VpnOptions
              · VpnConnection.create → TUN fd
              · protectProcessNet
                     │
                     ▼
              native startVpn(fd, optionsJson)
              · Client::setup_tun_fd_borrowed(fd)
              · thread: run_mainloop
                     │
              CommandHandle(Cancel) on disconnect / extension destroy
```

OpenConnect 9.20 由 `anyconnect-rs` 源码静态编译进 `libhanyconnect_ui.so`。
OHOS 交叉编译需要 NDK + 静态 libxml2；OpenSSL 默认走 `vendored-openssl`。

## 本地 ocserv 测试头端

完整协议联调可在开发机起一个 AnyConnect 兼容的 **ocserv**：

```bash
./scripts/dev-ocserv.sh start    # Docker；默认 demo/demo @ :4433
./scripts/dev-ocserv.sh url      # 例如 https://192.168.x.x:4433
./scripts/dev-ocserv.sh logs
./scripts/dev-ocserv.sh stop
```

| 项 | 值 |
| --- | --- |
| Server | `./scripts/dev-ocserv.sh url` 输出（手机填 **局域网 IP**，勿用 127.0.0.1） |
| Username / Password | `demo` / `demo`（`OCSERV_USER` / `OCSERV_PASS` 可改） |
| 证书 | 自签 → App 关闭严格证书信任 |
| 状态目录 | `.dev-ocserv/`（已 gitignore） |

真机与 Mac 需同一 Wi‑Fi（或 Mac 开热点）。成功 hilog：`anyconnect_obtain_cookie` → `anyconnect_cstp` → `anyconnect_setup_tun_fd` → `anyconnect_mainloop`。

## 设备 HAP（默认完整接入）

```bash
export OHOS_NDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony
export DEVECO_SDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk

# 默认 FEATURES=native-anyconnect，会 source env-ohos-anyconnect.sh
./scripts/package-hap.sh

# 仅 UI / 无 OpenConnect：
FEATURES= ./scripts/package-hap.sh
```

依赖：

| 依赖 | 来源 |
| --- | --- |
| OpenHarmony NDK | `OHOS_SDK_NATIVE` / `OHOS_NDK_HOME` |
| zlib | NDK sysroot（自动） |
| OpenSSL | `vendored-openssl`（默认）或 ohos-openssl prebuilt |
| libxml2 | anyconnect-rs `target/ohos-deps/…` 或 `third_party/libxml2-ohos-aarch64` |

连接行为：

| 环境变量 | 含义 |
| --- | --- |
| `HANYCONNECT_DRY_RUN=0`（默认，native 构建） | 真实 anyconnect-rs 会话 |
| `HANYCONNECT_DRY_RUN=1` | 跳过 headend，仅走生命周期 / 可选 VPN 扩展 |

## 主机协议 E2E

```bash
# 需要本机已安装 libxml2 / openssl 开发包（或 vendored-openssl）
./scripts/e2e-host-anyconnect.sh

# 可选：连真实 VPN
HANY_E2E_SERVER=https://vpn.example.com \
HANY_E2E_USER=alice \
HANY_E2E_PASSWORD='***' \
./scripts/e2e-host-anyconnect.sh
```

## 真机 / 模拟器 E2E

前置：`hdc list targets` 能看到设备；DevEco SDK / `ohrs` 可用。

```bash
export OHOS_NDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony
export DEVECO_SDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk

# 1) dry-run 自动连接（不依赖真实 headend，仍打包 native）
./scripts/e2e-device.sh --auto-connect --dry-run

# 2) 真实 headend（需设备支持 VPN Extension + 可达服务器）
# --accept-untrusted 仅用于自签名实验头端
./scripts/e2e-device.sh \
  --server vpn.corp.example \
  --user alice \
  --password '***' \
  --accept-untrusted \
  --auto-connect \
  --no-dry-run \
  --expect-connected \
  --allow-vpn-unsupported

# 3) 已有 HAP 时跳过编译
./scripts/e2e-device.sh --no-build --auto-connect --dry-run
```

### Want 参数

| 参数 | 含义 |
| --- | --- |
| `hanyServer` | 服务器 |
| `hanyName` | 配置显示名 |
| `hanyGroup` | 隧道组 |
| `hanyUsername` / `hanyPassword` | 账号 |
| `hanyAcceptUntrusted` | 仅测试私有/自签名头端的显式信任开关；默认仍严格校验证书 |
| `hanyAutoConnect` | 启动后自动连接 |
| `hanyDryRun` | dry-run（脚本默认 true；UI 真连默认 false） |
| `hanyExpectConnected` | 期望已连接 |
| `hanyExpectFailure` | 期望失败 |

手动：

```bash
hdc shell aa start -a EntryAbility -b com.richerfu.hanyconnect \
  --ps hanyServer vpn.example.com \
  --ps hanyAutoConnect true \
  --ps hanyDryRun true
```

### 成功标记（hilog）

脚本会在 `smoke-logs/e2e-device-*.log` 中查找：

- `configured native home` / `registered native platform callbacks`
- `e2e config applied` 或 `HAnyConnectE2E`
- `connect_auth_ok` / `session_connected`
- 真实路径：`backend_anyconnect` · `anyconnect_obtain_cookie` · `anyconnect_cstp` · `anyconnect_setup_tun_fd` · `anyconnect_mainloop`
- dry-run：`backend_dry_run`

## 会话生命周期

1. **UI prepare_connect** — `obtain_cookie`，把 cookie、组、身份和连接策略写入 handoff
2. **Extension prepare** — 使用 cookie 建立 CSTP，读取服务端地址、路由、DNS 和 MTU
3. **平台建 TUN** — ArkTS `VpnConnection.create`，把服务端下发配置写入系统
4. **attach_tun(fd)** — `setup_tun_fd_borrowed` + 后台线程 `run_mainloop`
5. **tick** — `Command::Statistics` 刷新 UI 流量
6. **disconnect / extension destroy** — `Command::Cancel`，join 主循环，销毁 TUN

## QEMU 网络验证

HDC 的 root shell 不受应用 UID 的 VPN 策略约束，不能用 root `ping` 判断应用是否走隧道。
使用仓库内探针降权到目标应用 UID 后再解析或连接：

```bash
OHOS_CLANG=/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony/native/llvm/bin/aarch64-unknown-linux-ohos-clang
"$OHOS_CLANG" scripts/device-net-probe.c -o smoke-logs/device-net-probe
hdc file send smoke-logs/device-net-probe /data/local/tmp/device-net-probe
hdc shell chmod 755 /data/local/tmp/device-net-probe

# 20010042 替换为 bm dump / ps 查到的应用 UID
hdc shell /data/local/tmp/device-net-probe 20010042 internal.corp.example
hdc shell /data/local/tmp/device-net-probe 20010042 10.10.10.1 443
```

同时在 headend 抓取 `vpns*` 流量，确认 `VpnConfig.dnsAddresses` 使用服务端下发的
DNS 地址。H-AnyConnect 的 `native-anyconnect` 会显式启用
`anyconnect/rediect-tun-dns`：当 Harmony 兼容环境保留上行解析器地址、但系统已经
把该 DNS 包路由进 VPN TUN 时，将目标改写到第一个 headend DNS；响应返回后再恢复
原解析器源地址并重算 IPv4/UDP 校验和。该 feature 不匹配域名、不硬编码客户网络，
未启用时 OpenConnect TUN 数据路径保持上游行为。
