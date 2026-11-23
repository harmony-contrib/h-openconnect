# H-OpenConnect Design Specification

This repository maintains both the design reference documentation and an interactive multi-device HTML design preview at [`docs/design/hopenconnect-nsh-design.html`](./hopenconnect-nsh-design.html).

## Visual Identity & Brand Icon (Proposal A — Concentric Portal)

- **Selected Icon**: **Proposal A (`Concentric Portal`)** — concentric open rings framing a central tunnel channel, matching the sibling **Paws** aesthetic (Paper soft gradient `#f8f9fb -> #eceef2` in light mode, `#1c2026 -> #15181e` in dark mode; `#4a840a` light stroke / `#c8f26d` dark stroke; `stroke-width: 1.9` on a 24×24 grid).
- **Canonical Design Assets**: Preserved in [`docs/design/icons/`](./icons/):
  - Layered SVGs: `background.svg`, `background-dark.svg`, `foreground.svg`, `foreground-dark.svg`, `startIcon-light.svg`, `startIcon-dark.svg`, `hopenconnect-icon.svg`
  - Raster exports (PNG & JPG): `hopenconnect-icon-{216,512,1024}.{png,jpg}`
- **HarmonyOS Project Assets**:
  - Layered adaptive icons & splash icons installed across `AppScope/resources/base/media/` and `entry/src/main/resources/{base,dark}/media/` (`background.png`, `foreground.png`, `startIcon.png`).
  - Splash window background synced in `entry/src/main/resources/{base,dark}/element/color.json` (`#F6F6F9` light / `#16191F` dark).

## Design Principles

1. **Dual Density Tiers & Multi-Device Form Factors (Paws Parity)**:
   - **Touch Tier (Phone & Pad)**: 40pt regular / 36pt small buttons, 46pt inputs & 44pt Select triggers, 46pt sheet rows / 36pt Pad popover rows. Pad stays on the Touch tier even with a keyboard attached (`!platform.desktop`).
   - **Pointer Tier (HarmonyOS 2-in-1 / PC)**: 28pt regular / 24pt small buttons, 32pt inputs & Select triggers, 26pt anchored menu rows, 40pt list rows, 7pt control / 10pt card radii.
   - **Phone (`< 600pt`, touch)**: 4-tab bottom navigation (`Overview / Connections / Statistics / Settings`), single-column content, native `BottomSheet`, Theme & Language as secondary push pages.
   - **Pad Portrait (`600–839pt`, touch)**: 4-tab bottom navigation (`maxWidth: 640`), anchored floating `<Popover>` for `Select`/`ObjectMenu` at `>= 768pt`, `BottomSheet` promoted to centered `Dialog` at `>= 768pt`.
   - **Pad Landscape (`>= 840pt`, touch)**: Left navigation rail (`64pt` for `840–1099pt`, `232pt` sidebar at `>= 1100pt`, toggleable), `48pt` unified toolbar aligned with status-bar safe area across sidebar & content, persistent 2-column Overview (`home-status-panel` + `home-profiles-panel` even when disconnected), Master-Detail Connections, 2-column Statistics, persistent split-pane Logs inspector, and unified Settings section bar (`Appearance` inline + `About`).
   - **HarmonyOS 2-in-1 / PC (`platform.desktop`)**: Seamless `48pt` window-draggable toolbar (`window.titleBar.style = "hidden"`), `titleBar().left/right` clearance for OS window buttons, left sidebar (`232pt` standard, `64pt` icon rail below `800pt` — never collapses to bottom tabs), `secondary` small toolbar buttons, right-click `ContextMenu`, closable `330pt` Logs inspector drawer + inline filter toolbar, keyboard shortcuts (`Mod+1..4`, `Mod+,`, `Mod+B`, `Mod+R`, `Mod+N`, `Escape`), single-instance `nsh.secondInstance` handling, and ScanKit camera scanning automatically hidden when unsupported (`canScanQr = false`).
2. **Overflow-Proof Controls**:
   - Multi-word 3–4 option pickers (such as Authentication Method and Split-Tunnel Mode in the Profile Editor) use the controlled `<Select>` component (native Bottom Sheet on Phone, anchored Popover on `>= 768pt`), preventing horizontal clipping on 390pt phones, English locale, and scaled typography.
   - Segmented controls are reserved for compact 2–5 short-label tabs (e.g. `Current / Archives`, `All / Info / Warn / Error / Debug`, `System / Light / Dark`).
3. **Logs & About Alignment (Paws Parity)**:
   - **Logs**: Header `Record/Stop` + `Clear`, `Current logs / Archives` tabs with archive count badge, compact single-line recording status note, search field + 5-level filter bar, virtualized auto-scrolling `<List>` with single-line floating jump-to-bottom pill (`↓ N new logs`), row tap detail sheet/inspector, and active-file delete guard on Archives.
   - **About & Privacy**: Brand hero (Icon A), version card with one-tap copy, Open Source Acknowledgements with external links, and dedicated `Privacy` sub-page.
4. **User-Facing Copy & Compliance**:
   - Zero internal engineering jargon or redundant filler copy.
   - Strict zero-"VPN" user-visible copy rule enforced statically by Rust contract tests (`startup_ui_contract.rs`), dynamically by `hopenconnect_app::sanitize_display_text`, and end-to-end across rendered screens in `ui/nsh/tests/app.test.ts` (zh-CN & en).
5. **Upstream Web Authentication**:
   - Interactive SAML/SSO web auth uses upstream `NativeAuthSession` from `nsh-platform-harmonyos` rather than a custom hand-written WebView sheet.
