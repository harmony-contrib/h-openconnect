# H-AnyConnect E2E

## Architecture (full anyconnect-rs)

```
UI (arkit) ──► hanyconnect_core::SessionEngine
                     │
                     │ prepare_connect (dry_run=false)
                     ▼
              anyconnect-rs Client
              · obtain_cookie
              · make_cstp_connection
              · setup_dtls (best-effort)
              · network_config → VpnOptions
                     │
                     ▼
              EntryAbility.requestStartVpn(optionsJson)
                     │
                     ▼
              HAnyConnectVpnExtensionAbility
              · VpnConnection.create → TUN fd
              · protectProcessNet
                     │
                     ▼
              native startVpn(fd, optionsJson)
              · Client::setup_tun_fd(fd)
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
./scripts/e2e-device.sh \
  --server vpn.corp.example \
  --user alice \
  --password '***' \
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
| `hanyAutoConnect` | 启动后自动连接 |
| `hanyDryRun` | dry-run（脚本默认 true；UI 真连默认 false） |
| `hanyExpectConnected` | 期望已连接 |
| `hanyExpectFailure` | 期望失败 |

手动：

```bash
hdc shell aa start -a EntryAbility -b com.southorange.hanyconnect \
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

1. **prepare_connect** — `obtain_cookie` + CSTP + 读取 `network_config`，**保留 Client**
2. **平台建 TUN** — ArkTS `VpnConnection.create`，把路由/DNS/地址写入系统
3. **attach_tun(fd)** — `setup_tun_fd` + 后台线程 `run_mainloop`
4. **tick** — `Command::Statistics` 刷新 UI 流量
5. **disconnect / extension destroy** — `Command::Cancel`，join 主循环，销毁 TUN
