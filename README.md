# H-AnyConnect

H-AnyConnect is a native HarmonyOS AnyConnect-compatible VPN client powered by
Rust, [Arkit](https://github.com/richerfu/arkit), and
[anyconnect-rs/OpenConnect](https://crates.io/crates/anyconnect).

The app performs interactive authentication in its UI process, hands the
authenticated session to a HarmonyOS `VpnExtensionAbility`, creates the system
TUN from the headend-provided network configuration, and runs the OpenConnect
packet loop inside the VPN extension process.

> H-AnyConnect is under active development. Validate routing, DNS, application
> binding, and reconnect behavior on a physical device or a VPN-enabled
> OpenHarmony QEMU image. A DevEco simulator can validate UI and profile flows,
> but does not prove that a system TUN was created or that traffic traversed it.

## Architecture

```text
Native ArkUI (Rust: Arkit + Dioxus)
    |  N-API
    v
EntryAbility (ArkTS)
    |  authentication UI, browser SSO, certificate picker
    |  start/stop VpnExtensionAbility
    v
HAnyConnectVpnExtensionAbility (ArkTS)
    |  VpnConnection.create(VpnConfig)
    |  per-socket protect + protectProcessNet()
    |  HarmonyOS-owned TUN fd
    v
hanyconnect_core (Rust)
    |  profile store, auth forms, session handoff, ashmem lifecycle IPC
    v
anyconnect crate / OpenConnect 9.20
    |  XML auth, cookie, CSTP, optional DTLS, routes, DNS, mainloop
    v
AnyConnect-compatible headend
```

The connection lifecycle is intentionally split across two application
processes:

1. The UI process calls `obtain_cookie`, handles group selection and all
   interactive authentication forms, then writes a short-lived private session
   handoff.
2. The VPN extension resumes the authenticated cookie, establishes CSTP, and
   reads the assigned addresses, routes, DNS servers, search domains, and MTU.
3. ArkTS maps that headend configuration directly to HarmonyOS `VpnConfig` and
   creates the system TUN.
4. Rust attaches the TUN fd to OpenConnect, optionally enables DTLS, and runs
   the packet mainloop.
5. The UI observes extension-owned lifecycle and traffic statistics through
   checksummed, double-buffered ashmem frames with socket change notifications.

Missing or malformed headend network configuration fails the connection. The
app does not synthesize fallback tunnel addresses, public DNS servers, or
default routes.

## Features

- **Standards-aligned AnyConnect authentication**
  - Fetches the initial authentication form when a server is entered and
    presents every group advertised by the headend.
  - Sends the selected protocol group through OpenConnect's standard
    `group-select` flow.
  - Supports multi-round username/password, AAA/RADIUS challenge, SMS OTP,
    token, and select fields.
  - Keeps challenge text inputs visible so verification codes can be reviewed
    while typing.
- **Enterprise authentication**
  - External-browser SAML/SSO-v2 with the system browser and OpenConnect's local
    callback listener.
  - RSA SecurID and TOTP software-token modes.
  - Password, certificate, password-plus-certificate, and SAML profile modes.
- **Certificate and TLS policy**
  - System trust, private CA files, server certificate pins, and explicit
    development-only untrusted-certificate policy.
  - PEM, DER, and PKCS#12 client certificates, private keys, secondary
    certificates, and key passwords.
  - Certificate files selected through the HarmonyOS document picker are copied
    into the application-private sandbox.
- **VPN data path**
  - Headend-provided IPv4/IPv6 addresses, split includes, split excludes, DNS
    servers, search domains, and MTU.
  - Split tunnel, force-global routing, local-LAN exclusions, and trusted or
    blocked application lists.
  - CSTP transport with optional DTLS acceleration and CSTP fallback.
  - Per-socket protection before connecting to the VPN gateway, followed by
    process-network protection after TUN creation.
  - Feature-gated TUN DNS redirection for HarmonyOS environments whose system
    resolver keeps the uplink DNS destination after routing the packet into the
    VPN.
- **Connection resilience**
  - Ordered backup gateways for eligible network, TLS, and connection failures.
  - OpenConnect mainloop reconnect plus profile-controlled reconnect after an
    unexpected disconnect while the app is active.
  - Session-scoped ashmem state disappears with its owning processes, so stale
    files or PID reuse cannot revive a false connected session.
- **Native application experience**
  - Native ArkUI interface for phone, tablet, and 2-in-1 targets.
  - Connection profiles, favorites, live status, traffic statistics,
    diagnostics, and light/dark appearance.
  - Opt-in diagnostic recording with bounded live history, UTC daily archives,
    system document export, and protected deletion of inactive archives.
  - English and Simplified Chinese UI.
  - Persisted profile selection, language, theme, and credentials.

Profile data is stored in the application-private directory with restricted
permissions and is excluded from HarmonyOS backup. Production abilities do not
accept credentials, trust overrides, or auto-connect instructions through
`Want` parameters.

## Open source

The complete source for H-AnyConnect is published at
[harmony-contrib/h-anyconnect](https://github.com/harmony-contrib/h-anyconnect).
The application is available under your choice of the MIT license or Apache
License 2.0.

The release build includes the `anyconnect` Rust wrapper and a statically linked
OpenConnect 9.20 library. OpenConnect is licensed under LGPL-2.1-only; its exact
pinned source and relinkable build inputs are distributed by
[`anyconnect-sys` 0.1.0](https://crates.io/crates/anyconnect-sys/0.1.0).
See [open-source notices](OPEN_SOURCE.md) for component versions, source
locations, licenses, and rebuild information.

H-AnyConnect is an independent project and is not affiliated with or endorsed
by Cisco. Cisco and AnyConnect names and trademarks belong to their respective
owners.

## Building

### Prerequisites

- macOS with DevEco Studio and the HarmonyOS SDK installed.
- Rust 1.89 or a compatible newer toolchain.
- [`ohrs`](https://github.com/ohos-rs/ohos-rs) available in `PATH`.
- `hvigorw`, `ohpm`, and `hdc`; the scripts can use the copies bundled with
  DevEco Studio.
- A HarmonyOS signing profile when producing a HAP for a physical device.
- Docker Desktop only when using the optional local `ocserv` headend.

The project targets HarmonyOS 6.1.1 / API 24, is compatible with HarmonyOS
6.0.2, and currently packages an `arm64-v8a` native library.

The default native build uses:

| Dependency             | Source                                                       |
| ---------------------- | ------------------------------------------------------------ |
| AnyConnect/OpenConnect | crates.io `anyconnect 0.1.0`, OpenConnect 9.20               |
| Native UI              | `richerfu/arkit` commit `9f15744`                            |
| TLS                    | Vendored OpenSSL                                             |
| XML                    | Static OHOS libxml2 under `third_party/libxml2-ohos-aarch64` |

If the static libxml2 prefix is missing, `scripts/build-libxml2-ohos.sh` builds
it for `aarch64-unknown-linux-ohos`.

### Build an unsigned release HAP

```sh
export OHOS_NDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony
export DEVECO_SDK_HOME=/Applications/DevEco-Studio.app/Contents/sdk

scripts/package-hap.sh
```

Output:

```text
entry/build/default/outputs/default/entry-default-unsigned.hap
```

`package-hap.sh` enables `native-anyconnect` by default. A UI-only development
shell can be built explicitly with:

```sh
FEATURES= scripts/package-hap.sh
```

The UI-only build does not provide a real OpenConnect tunnel and must not be
used for VPN acceptance testing.

### Manual native build

```sh
. scripts/env-ohos-anyconnect.sh
ohrs build --arch aarch --release -p hanyconnect_ui -- \
  --features native-anyconnect
```

## Install and launch

List connected targets:

```sh
hdc list targets
```

After configuring a valid HarmonyOS signing profile, install the signed HAP and
launch the application:

```sh
hdc install -r \
  entry/build/default/outputs/default/entry-default-signed.hap
hdc shell aa start -b com.richerfu.hanyconnect -a EntryAbility
```

For a target selected by key, add `-t <target-key>` immediately after `hdc`.

The launch smoke script can build, install, start `EntryAbility`, and capture
`hilog`:

```sh
scripts/e2e-device.sh --target <target-key>
```

Its default unsigned HAP is intended for compatible development/QEMU
environments. Physical devices normally require a correctly signed HAP.

## Local AnyConnect headend

The repository includes a Docker-based `ocserv` environment for protocol
development:

```sh
scripts/dev-ocserv.sh start
scripts/dev-ocserv.sh url
scripts/dev-ocserv.sh logs
scripts/dev-ocserv.sh stop
```

The default account is `demo` / `demo`. A physical device must use the
development host's LAN address instead of `127.0.0.1`. The generated server
certificate is self-signed; disable strict certificate trust only for this
isolated development profile.

## OpenHarmony QEMU

Use a standard-system image that includes VPN Manager, `VpnExtension`,
`/dev/tun`, policy routing, SettingsData, and the system VPN authorization
dialog. The [ohos-qemu](https://github.com/harmony-contrib/ohos-qemu) project
provides suitable prebuilt images.

After starting QEMU, connect the forwarded HDC endpoint:

```sh
hdc tconn 127.0.0.1:5555
hdc list targets
scripts/e2e-device.sh --target 127.0.0.1:5555
```

Approve the system VPN authorization dialog on first use. A successful app
launch alone is not a tunnel test: create a real connection profile, connect to
an accessible headend, and verify DNS plus TCP traffic under the application
UID.

Root `hdc shell` traffic is not necessarily subject to the application's VPN
policy. The included probe can run with the target application UID:

```sh
OHOS_CLANG=/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony/native/llvm/bin/aarch64-unknown-linux-ohos-clang
"$OHOS_CLANG" scripts/device-net-probe.c -o smoke-logs/device-net-probe
hdc file send smoke-logs/device-net-probe /data/local/tmp/device-net-probe
hdc shell chmod 755 /data/local/tmp/device-net-probe

# Replace 20010042 with the UID reported for com.richerfu.hanyconnect.
hdc shell /data/local/tmp/device-net-probe 20010042 internal.example
hdc shell /data/local/tmp/device-net-probe 20010042 10.10.10.1 443
```

## Tests

Run the core unit and integration tests against the published AnyConnect crate:

```sh
cargo test -p hanyconnect_core --features native-anyconnect
```

Run the host-side AnyConnect checks, optionally against a real headend:

```sh
scripts/e2e-host-anyconnect.sh

HANY_E2E_SERVER=https://vpn.example.com \
HANY_E2E_USER=alice \
HANY_E2E_PASSWORD='***' \
scripts/e2e-host-anyconnect.sh
```

Build the full native library and release HAP:

```sh
scripts/package-hap.sh
```

See [end-to-end and device validation](docs/e2e.md) for the complete lifecycle,
QEMU network probe, and DNS verification procedure. See
[UI and protocol mapping](docs/ui-map.md) for the user-flow-to-implementation
map.

## Project structure

```text
AppScope/                       Application identity and launcher resources
entry/                          HarmonyOS application module
  src/main/ets/                 Entry, backup, and VPN extension abilities
  src/main/cpp/types/           Generated N-API declarations
  src/main/resources/           UI resources and backup policy
crates/
  hanyconnect_core/             Profiles, auth, OpenConnect, VPN lifecycle
  hanyconnect_ui/               Native ArkUI interface and N-API exports
scripts/                        Build, package, ocserv, smoke, and probe tools
docs/                           Architecture, UI mapping, and validation notes
third_party/                    OHOS static native dependencies
```

## License

[MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE). Third-party components
remain under their respective licenses; see [OPEN_SOURCE.md](OPEN_SOURCE.md).
