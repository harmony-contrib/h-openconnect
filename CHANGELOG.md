# Changelog

All notable changes to H-OpenConnect are documented in this file.

## [1.0.1] - 2026-09-16

### Changed

- Refreshed the native interface with the upgraded Arkit and shadcn component
  foundation.
- Aligned VPN lifecycle and state synchronization with the extension-owned
  runtime state.

### Fixed

- Prevented stale VPN ownership and process state from being reported as an
  active connection.
- Corrected route policy, session handoff, reconnect, and disconnect behavior.
- Kept multi-round authentication input forms visible above the software
  keyboard in the bottom sheet.
