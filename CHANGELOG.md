# Changelog

All notable changes to H-OpenConnect are documented in this file.

## [2.0.0] - 2026-11-24

### Added

- Rebuilt the native HarmonyOS interface on the NSH runtime with the Paper +
  Citrus Lime visual system and the Concentric Portal brand icon.
- Added full Pad (portrait and landscape dual-pane) and HarmonyOS 2-in-1 PC
  desktop support with pointer-density styling, seamless draggable title bar,
  keyboard shortcuts, context menus, and closable log inspector drawer.
- Added profile QR code export and camera scan import, favorites filtering,
  and dedicated diagnostic log recording and archive management.

### Changed

- Migrated interactive SAML/SSO web authentication to upstream native auth
  session handling.
- Refined disconnect, reconnect, and cold-start hydration transitions to avoid
  transient UI flicker and stale connected indicators.

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
