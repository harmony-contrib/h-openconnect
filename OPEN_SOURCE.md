# Open-source notices

H-AnyConnect is open source under your choice of the
[MIT license](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE). Its
corresponding source is available from:

https://github.com/harmony-contrib/h-anyconnect

The exact Rust dependency graph for a release is recorded in `Cargo.lock`.
License files and notices supplied by each Rust package remain part of that
package's source distribution.

## Major components

| Component | Version or revision | License | Source |
| --- | --- | --- | --- |
| anyconnect | 0.1.0 | MIT OR Apache-2.0 | https://github.com/networks-rs/anyconnect-rs |
| anyconnect-sys / OpenConnect | 0.1.0 / 9.20 (`8ae87c089bac597d9e09902bbedd03e0c45d8269`) | LGPL-2.1-only | https://github.com/networks-rs/anyconnect-rs |
| Arkit | `765b1f4ff591fcd02af6fdbc115d28d297f70d6a` | MIT OR Apache-2.0 | https://github.com/richerfu/arkit |
| Dioxus | 0.7.9 | MIT OR Apache-2.0 | https://github.com/DioxusLabs/dioxus |
| ohos-rs bindings | versions locked in `Cargo.lock` | MIT or MIT OR Apache-2.0 | https://github.com/ohos-rs |
| OpenSSL | 3.6.3 | Apache-2.0 | https://github.com/openssl/openssl |
| libxml2 | 2.12.9 | MIT | https://gitlab.gnome.org/GNOME/libxml2 |

HarmonyOS platform libraries referenced dynamically by the HAP are supplied by
the operating system and are not redistributed by this repository.

## OpenConnect and LGPL relinking

The release feature statically links OpenConnect 9.20 through
`anyconnect-sys`. The matching crate package contains the complete pinned
OpenConnect source, its LGPL license text, provenance, and the generated build
inputs required by the supported source build:

https://crates.io/crates/anyconnect-sys/0.1.0

The public H-AnyConnect source and the build instructions in `README.md`
provide the application material needed to rebuild the native library and HAP
with a modified compatible OpenConnect build. The application does not impose
additional restrictions on modification or reverse engineering performed for
debugging changes to the LGPL-covered library.

To rebuild against a modified OpenConnect 9.20-compatible source tree, keep the
normal HarmonyOS build environment from `README.md` and set the source override:

```sh
ANYCONNECT_SOURCE_DIR=/absolute/path/to/openconnect \
  scripts/package-hap.sh
```

`anyconnect-sys` also accepts `ANYCONNECT_LIB_DIR` when linking an already-built
compatible OpenConnect library. Its package README documents the complete
source and library override contract.

## Privacy

Connection profiles and credentials are stored in the application-private
directory with restricted permissions and are excluded from HarmonyOS backup.
Diagnostic recording is disabled by default and writes local daily archives
only after the user enables it. H-AnyConnect does not contain analytics or
telemetry upload code; network requests are initiated for the VPN gateways and
authentication services configured by the user.

## Trademarks

H-AnyConnect is an independent project and is not affiliated with or endorsed
by Cisco. Cisco and AnyConnect names and trademarks belong to their respective
owners.
