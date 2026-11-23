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

1. **Dual Density Tiers**:
   - **Touch Tier (Phone & Pad)**: 40pt regular / 36pt small buttons, 46pt inputs & Select triggers, 46pt sheet rows / 36pt Pad popover rows.
   - **Pointer Tier (HarmonyOS 2-in-1 / PC)**: 28pt regular / 24pt small buttons, 32pt inputs & Select triggers, 26pt anchored menu rows.
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
