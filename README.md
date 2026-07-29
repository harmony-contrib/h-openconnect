# H-AnyConnect

HarmonyOS 上的 AnyConnect 兼容 VPN 客户端（Arkit + shadcn + **anyconnect-rs / OpenConnect 9.20**）。

**真实接通**（非 preview / mock）：

1. UI：`obtain_cookie` → 写出 `session-handoff.json`（含密码，仅本机沙箱）
2. 系统 `VpnExtensionAbility`（独立进程）只使用认证 cookie 建立 CSTP
3. 用真实分配地址创建 TUN → `setup_tun_fd` + `run_mainloop`
4. UI 经 `platform-vpn-state.json` 同步 Connected（对齐 paws）
5. 断开：停扩展 + cancel mainloop

## 结构

```
crates/hanyconnect_core/   # 会话引擎 + anyconnect-rs
crates/hanyconnect_ui/     # Arkit UI + NAPI (cdylib)
entry/                     # HarmonyOS 壳 (ArkTS + VPN Extension)
scripts/                   # OHOS 依赖 / 打包 / E2E
```

## 页面

| 底部导航 | 路由 | 说明 |
| --- | --- | --- |
| 连接 | `/` | 连接/断开、状态、会话摘要 |
| 配置 | `/connections` | 连接列表、收藏、编辑/删除 |
| 统计 | `/statistics` | 时长 / 流量 / IP / MTU（OpenConnect stats） |
| 更多 | `/more` | 外观、诊断、关于 |

## 构建（设备 HAP，默认链 OpenConnect）

```bash
export OHOS_NDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony
export DEVECO_SDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk

# 自动：libxml2 前缀 + vendored OpenSSL + --features native-anyconnect
./scripts/package-hap.sh

# 安装并启动
./scripts/run-simulator.sh
```

仅 UI 壳（不链 OpenConnect）：

```bash
FEATURES= ./scripts/package-hap.sh
```

手动：

```bash
. ./scripts/env-ohos-anyconnect.sh
ohrs build --arch aarch --release -p hanyconnect_ui -- --features native-anyconnect
```

## 连接行为

| 构建 | 默认 dry-run | 说明 |
| --- | --- | --- |
| `--features native-anyconnect`（默认打包） | **false** | 真实 headend + TUN mainloop |
| 无 feature | true | 平台编排 / mock 生命周期 |
| `HANYCONNECT_DRY_RUN=1` | 强制 dry-run | E2E 不连 headend |

在「配置」页填好服务器后会自动读取首次 AnyConnect 认证表单并展示全部
auth group；选定的协议值会在正式认证的首个 XML POST 中作为
`<group-select>` 发送。填写用户/密码后，主页点连接即可走完整协议。

## 本地测试 VPN（ocserv）

真连协议需要 AnyConnect 兼容 headend。本仓库提供一键本地服务：

```bash
# 需 Docker Desktop 运行中
./scripts/dev-ocserv.sh start
# 打印 Server URL / demo 账号；手机填局域网 IP，不要用 127.0.0.1

./scripts/dev-ocserv.sh status
./scripts/dev-ocserv.sh logs
./scripts/dev-ocserv.sh stop
```

默认：`https://<本机局域网IP>:4433`，用户 `demo` / 密码 `demo`。  
App 内需同时关闭「严格证书信任」和「阻止不可信服务器」（仅限该自签名开发环境）。

## 测试

```bash
# 正式 HAP 的构建、安装和启动检查
./scripts/e2e-device.sh

# 主机协议（可选 live headend）
./scripts/e2e-host-anyconnect.sh
```

设备连接场景通过真实 UI 配置执行；正式 Ability 不提供密码或自动连接 Want 注入入口。

详见 [docs/e2e.md](docs/e2e.md)。

## Arkit

本地依赖：`../../ohos-rs/arkit`（CSS-style RSX）。

| 旧写法 | 新写法 |
| --- | --- |
| `percent_width: 1.0` | `width: "100%"` |
| `alignment: 0` | `alignment: "top_start"` |

## 真机签名

DevEco → **Signing Configs → Fix**（bundle `com.southorange.hanyconnect`）。

## 图标

分层图标在 `AppScope/resources/base/media/` 与 `entry/src/main/resources/base/media/`。
