import { expect, open, test } from "nsh/test";
import { MockHOpenHost } from "./mock-host";

const VPN_WORD_RE = /\bvpn\b/i;

function assertNoVpnCopy(texts: string[], contextLabel: string): void {
  const offenders = texts.filter((text) => VPN_WORD_RE.test(text));
  if (offenders.length > 0) {
    throw new Error(
      `${contextLabel} exposed forbidden VPN term: ${offenders.join(" | ")}`,
    );
  }
}

test("responsive navigation works across Phone, Pad and Desktop viewports", async () => {
  const phoneHost = new MockHOpenHost({ language: "zh-CN" });
  const phone = await open("../App.tsx", {
    platform: "ios",
    width: 390,
    height: 844,
    locale: "zh-CN",
    host: phoneHost.hostOptions(),
  });
  phoneHost.attachApp(phone);
  await phone.waitFor("id:home-status-card");
  expect(phone.shows("id:bottom-navigation")).toBe(true);
  expect(phone.shows("id:desktop-sidebar")).toBe(false);

  await phone.press("id:nav-profiles");
  await phone.waitFor("id:profiles-list");
  expect(phone.shows("id:profiles-qr-migration-card")).toBe(true);
  await phone.press("id:profiles-qr-migration-card");
  await phone.waitFor("id:export-profile-qr");
  await phone.press("id:dialog-close");
  await phone.waitFor("id:export-profile-qr", { state: "gone" });
  await phone.press("id:nav-statistics");
  await phone.waitFor("id:stats-traffic-card");
  await phone.press("id:nav-settings");
  await phone.waitFor("id:settings-open-logs");
  await phone.press("id:nav-home");
  await phone.waitFor("id:home-status-card");

  const padHost = new MockHOpenHost({ language: "zh-CN" });
  const pad = await open("../App.tsx", {
    platform: "ios",
    width: 1024,
    height: 768,
    locale: "zh-CN",
    host: padHost.hostOptions(),
  });
  padHost.attachApp(pad);
  await pad.waitFor("id:home-status-card");
  expect(pad.shows("id:desktop-sidebar")).toBe(true);
  expect(pad.shows("id:bottom-navigation")).toBe(false);

  await pad.setViewport(420, 844);
  await pad.waitFor("id:bottom-navigation");
  expect(pad.shows("id:desktop-sidebar")).toBe(false);

  const pcHost = new MockHOpenHost({ language: "zh-CN" });
  const pc = await open("../App.tsx", {
    platform: "windows",
    width: 1280,
    height: 900,
    locale: "zh-CN",
    host: pcHost.hostOptions(),
  });
  pcHost.attachApp(pc);
  await pc.waitFor("id:home-status-card");
  expect(pc.shows("id:desktop-sidebar")).toBe(true);
  expect(pc.shows("id:bottom-navigation")).toBe(false);
  await pc.press("id:desktop-toggle-sidebar");
  expect(pc.shows("id:desktop-sidebar")).toBe(true);
  await pc.keyboard.press("mod+2");
  await pc.waitFor("id:profiles-list");
  await pc.keyboard.press("mod+4");
  await pc.waitFor("id:logs-list");
});

test("connect -> MFA challenge -> connected stats -> disconnect flow", async () => {
  const host = new MockHOpenHost({ language: "zh-CN" });
  const app = await open("../App.tsx", {
    platform: "ios",
    width: 390,
    height: 844,
    locale: "zh-CN",
    host: host.hostOptions(),
  });
  host.attachApp(app);
  await app.waitFor("id:home-connection-action");

  await app.press("id:home-connection-action");
  await app.waitFor("id:auth-challenge-form");
  expect(app.texts().some((t) => t.includes("正在验证身份…"))).toBe(true);
  expect(app.texts().some((t) => t.includes("gw.example.com"))).toBe(true);
  expect(app.shows("id:auth-prompt-banner")).toBe(true);
  expect(app.shows("id:auth-field-0")).toBe(true);
  await app.type("id:auth-field-0", "123456");
  await app.press("id:auth-submit");

  await app.waitFor("id:auth-challenge-form", { state: "gone" });
  await app.waitFor("id:home-session-card");
  expect(app.texts().some((t) => t.includes("10.24.0.15"))).toBe(true);

  await app.press("id:home-open-stats");
  await app.waitFor("id:stats-network-card");
  expect(app.shows("id:stats-window-meta")).toBe(true);
  expect(app.shows("id:stats-transport-badge")).toBe(true);
  expect(app.texts().some((t) => t.includes("DTLS 已启用"))).toBe(true);
  expect(app.texts().some((t) => t.includes("10.24.0.15"))).toBe(true);
  expect(app.texts().some((t) => t.includes("1400"))).toBe(true);

  await app.press("id:nav-home");
  await app.waitFor("id:home-connection-action");
  await app.press("id:home-connection-action");
  await app.waitUntil(() =>
    host.commands.some((c) => c.command === "vpn.disconnect"),
  );
  await app.waitFor("id:home-session-card", { state: "gone" });
});

test("profile CRUD, dirty draft leave guard and delete confirmation", async () => {
  const host = new MockHOpenHost({ language: "zh-CN" });
  const app = await open("../App.tsx", {
    platform: "windows",
    width: 1280,
    height: 900,
    locale: "zh-CN",
    host: host.hostOptions(),
  });
  host.attachApp(app);
  await app.waitFor("id:desktop-sidebar");

  await app.press("id:nav-profiles");
  await app.waitFor("id:profiles-list");
  expect(app.shows("id:profile-row-hq-gw")).toBe(true);

  await app.press("id:profile-star-lab-sg");
  await app.waitUntil(() =>
    host.commands.some(
      (c) =>
        c.command === "profiles.toggleFavorite" && c.payload.id === "lab-sg",
    ),
  );

  await app.press("id:header-add-profile");
  await app.waitFor("id:editor-name");
  await app.type("id:editor-name", "Tokyo Edge");
  await app.type("id:editor-server", "tokyo.example.com");
  await app.type("id:editor-username", "alice");
  await app.type("id:editor-password", "secret");
  // Typing server & credentials must NOT auto-trigger group discovery.
  expect(
    host.commands.some((c) => c.command === "profiles.discoverGroups"),
  ).toBe(false);
  expect(app.shows("id:editor-group")).toBe(true);
  // Clicking the explicit refresh icon loads groups on demand.
  await app.press("id:editor-refresh-groups");
  await app.waitUntil(() =>
    host.commands.some((c) => c.command === "profiles.discoverGroups"),
  );
  await app.waitFor("id:editor-group-select");

  app.onDialog(() => "继续编辑");
  await app.press("id:nav-home");
  expect(app.dialogs().length).toBeGreaterThanOrEqual(1);
  expect(app.shows("id:editor-name")).toBe(true);

  await app.press("id:editor-save");
  await app.waitUntil(() =>
    host.commands.some((c) => c.command === "profiles.save"),
  );
  await app.waitFor("id:profiles-list");
  expect(app.texts().some((t) => t.includes("Tokyo Edge"))).toBe(true);

  await app.press("id:profile-row-lab-sg");
  await app.waitFor("id:detail-qr-profile");
  await app.press("id:detail-qr-profile");
  await app.waitFor("id:export-profile-qr");
  assertNoVpnCopy(app.texts(), "zh-CN/qr-dialog");
  await app.press("id:export-profile-qr");
  await app.waitUntil(() => host.exportedQrs.length === 1);
  await app.press("id:dialog-close");
  await app.waitFor("id:export-profile-qr", { state: "gone" });

  // Desktop 2-in-1 hides camera-only QR scanning cleanly when unsupported.
  expect(app.shows("id:header-scan-qr")).toBe(false);

  await app.press("id:profile-row-lab-sg");
  await app.waitFor("id:detail-delete-profile");
  app.onDialog((dlg) => dlg.buttons[dlg.buttons.length - 1]);
  await app.press("id:detail-delete-profile");
  await app.waitUntil(() =>
    host.commands.some(
      (c) => c.command === "profiles.delete" && c.payload.id === "lab-sg",
    ),
  );
  await app.waitFor("id:profile-row-lab-sg", { state: "gone" });
});

test("Pad portrait & landscape align with Paws touch-tier split-pane, popovers, and unified settings", async () => {
  const host = new MockHOpenHost({ language: "zh-CN" });
  const pad = await open("../App.tsx", {
    platform: "ios",
    width: 1024,
    height: 768,
    locale: "zh-CN",
    host: host.hostOptions({
      seedInitialDomains: true,
      deviceType: "tablet",
      canScanQr: true,
    }),
  });
  host.attachApp(pad);

  // Pad landscape (1024 >= 840): sidebar rail + always-2-col overview even when disconnected.
  await pad.waitFor("id:wide-home-workspace");
  expect(pad.shows("id:desktop-sidebar")).toBe(true);
  expect(pad.shows("id:bottom-navigation")).toBe(false);
  expect(pad.shows("id:home-status-panel")).toBe(true);
  expect(pad.shows("id:home-profiles-panel")).toBe(true);
  expect(pad.shows("id:header-refresh")).toBe(true);
  assertNoVpnCopy(pad.texts(), "pad/home");

  // Pad sidebar reflects authenticating status while challenge is active.
  await pad.press("id:home-connection-action");
  await pad.waitFor("id:auth-challenge-form");
  expect(pad.texts().some((t) => t.includes("正在验证"))).toBe(true);
  await pad.press("id:auth-cancel");
  await pad.waitFor("id:auth-challenge-form", { state: "gone" });

  // Profiles master-detail + camera QR scan button available on tablet.
  await pad.press("id:nav-profiles");
  await pad.waitFor("id:profiles-master-detail");
  expect(pad.shows("id:profile-detail-card")).toBe(true);
  expect(pad.shows("id:profiles-qr-migration-card")).toBe(false);
  expect(pad.shows("id:header-scan-qr")).toBe(true);
  await pad.press("id:header-scan-qr");
  await pad.waitUntil(() =>
    host.commands.some((c) => c.command === "profiles.importQr"),
  );
  expect(pad.texts().some((t) => t.includes("Scanned Branch"))).toBe(true);

  // Editor Select opens anchored Popover on Pad (>= 768pt) and survives resizing to Pad portrait.
  await pad.press("id:header-add-profile");
  await pad.waitFor("id:editor-name");
  await pad.type("id:editor-name", "Pad Draft");
  await pad.press("id:editor-auth-method");
  await pad.waitFor("id:select-popover");
  expect(pad.shows("id:select-option-saml")).toBe(true);
  await pad.keyboard.press("escape");
  await pad.waitFor("id:select-popover", { state: "gone" });

  // Resize from Pad landscape (1024) to Pad portrait (768) preserves form draft.
  await pad.setViewport(768, 1024);
  expect(pad.value("id:editor-name")).toBe("Pad Draft");
  expect(pad.shows("id:desktop-sidebar")).toBe(false);

  // Dismiss draft and return to Pad landscape.
  pad.onDialog((dlg) => dlg.buttons[dlg.buttons.length - 1]);
  await pad.press("id:nav-back");
  await pad.waitFor("id:profiles-list");
  expect(pad.shows("id:bottom-navigation")).toBe(true);

  await pad.setViewport(1180, 820);
  await pad.waitFor("id:desktop-sidebar");

  // Pad landscape Statistics 2-col layout.
  await pad.press("id:nav-statistics");
  await pad.waitFor("id:statistics-wide-workspace");
  assertNoVpnCopy(pad.texts(), "pad/statistics");

  // Pad landscape Logs persistent split-pane inspector.
  await pad.press("id:nav-logs");
  await pad.waitFor("id:logs-list");
  await pad.waitFor("id:log-inspector-copy");
  assertNoVpnCopy(pad.texts(), "pad/logs");

  // Pad landscape Settings unified section bar (Appearance inline + About tab).
  await pad.press("id:nav-settings");
  await pad.waitFor("id:settings-section-bar");
  expect(pad.shows("id:pref-theme-dark")).toBe(true);
  expect(pad.shows("id:pref-language-en")).toBe(true);
  assertNoVpnCopy(pad.texts(), "pad/settings");
  await pad.press("id:settings-section-about");
  await pad.waitFor("id:about-version-card");
  expect(pad.shows("id:settings-section-bar")).toBe(true);
  assertNoVpnCopy(pad.texts(), "pad/about");
});

test("PC desktop shell, shortcuts, context menu, narrow-window rail, and closable log inspector", async () => {
  const host = new MockHOpenHost({ language: "zh-CN" });
  const pc = await open("../App.tsx", {
    platform: "windows",
    width: 1280,
    height: 900,
    locale: "zh-CN",
    host: host.hostOptions({
      seedInitialDomains: true,
      deviceType: "2in1",
      canScanQr: false,
    }),
  });
  host.attachApp(pc);

  // PC standard (1280x900): sidebar + 2-col overview even when disconnected.
  await pc.waitFor("id:desktop-home-workspace");
  expect(pc.shows("id:desktop-sidebar")).toBe(true);
  expect(pc.shows("id:bottom-navigation")).toBe(false);
  expect(pc.shows("id:header-refresh")).toBe(true);
  assertNoVpnCopy(pc.texts(), "pc/home");

  // Keyboard shortcuts: Mod+2 -> Profiles, right-click ContextMenu -> Edit.
  await pc.keyboard.press("mod+2");
  await pc.waitFor("id:profiles-master-detail");
  expect(pc.shows("id:header-scan-qr")).toBe(false);
  await pc.rightClick("id:profile-row-lab-sg");
  await pc.waitFor("id:profile-edit-lab-sg");
  await pc.press("id:profile-edit-lab-sg");
  await pc.waitFor("id:editor-name");
  await pc.type("id:editor-name", " Updated");

  // Narrow PC window (620x760 & 390x844) NEVER collapses to bottom tabs and keeps draft intact.
  await pc.setViewport(620, 760);
  expect(pc.shows("id:desktop-sidebar")).toBe(true);
  expect(pc.shows("id:bottom-navigation")).toBe(false);
  expect(pc.value("id:editor-name")).toContain("Updated");

  await pc.setViewport(390, 844);
  expect(pc.shows("id:desktop-sidebar")).toBe(true);
  expect(pc.shows("id:bottom-navigation")).toBe(false);

  // Restore to 1280x900 and discard draft via nav-back.
  await pc.setViewport(1280, 900);
  pc.onDialog((dlg) => dlg.buttons[dlg.buttons.length - 1]);
  await pc.press("id:nav-back");
  await pc.waitFor("id:profiles-master-detail");

  // Mod+N opens new profile editor directly on PC.
  await pc.keyboard.press("mod+n");
  await pc.waitFor("id:editor-name");
  expect(pc.value("id:editor-name")).toBe("");
  await pc.press("id:nav-back");
  await pc.waitFor("id:profiles-master-detail");

  // Mod+4 -> Logs with closable 330pt inspector drawer.
  await pc.keyboard.press("mod+4");
  await pc.waitFor("id:logs-list");
  expect(pc.shows("id:logs-inspector")).toBe(false);
  await pc.press("id:log-row-2026-11-23T10:00:09Z");
  await pc.waitFor("id:logs-inspector");
  await pc.press("id:log-inspector-copy");
  expect(pc.clipboard()).toContain("Gateway rejected expired token");
  await pc.press("id:dialog-close");
  await pc.waitFor("id:logs-inspector", { state: "gone" });
  assertNoVpnCopy(pc.texts(), "pc/logs");

  // Mod+, -> Unified Settings section bar on PC.
  await pc.keyboard.press("mod+,");
  await pc.waitFor("id:settings-section-bar");
  expect(pc.shows("id:pref-theme-dark")).toBe(true);
  expect(pc.shows("id:pref-language-en")).toBe(true);
  await pc.press("id:settings-section-about");
  await pc.waitFor("id:about-version-card");
  assertNoVpnCopy(pc.texts(), "pc/settings-about");

  // Mod+B toggles sidebar without losing the sidebar chrome.
  await pc.keyboard.press("mod+b");
  expect(pc.shows("id:desktop-sidebar")).toBe(true);
  await pc.keyboard.press("mod+1");
  await pc.waitFor("id:desktop-home-workspace");
});

test("Paws-aligned logs page: recording, level filter, inspector, archives and clear", async () => {
  const host = new MockHOpenHost({ language: "zh-CN" });
  const app = await open("../App.tsx", {
    platform: "ios",
    width: 390,
    height: 844,
    locale: "zh-CN",
    host: host.hostOptions(),
  });
  host.attachApp(app);
  await app.waitFor("id:nav-settings");

  await app.press("id:nav-settings");
  await app.waitFor("id:settings-open-logs");
  await app.press("id:settings-open-logs");
  await app.waitFor("id:logs-list");

  await app.press("id:logs-toggle-recording");
  await app.waitUntil(() =>
    host.commands.some(
      (c) => c.command === "logs.setRecording" && c.payload.enabled === true,
    ),
  );

  await app.press("id:logs-level-error");
  await app.waitFor("id:log-row-2026-11-23T10:00:09Z");
  expect(app.shows("id:log-row-2026-11-23T10:00:01Z")).toBe(false);

  await app.press("id:log-row-2026-11-23T10:00:09Z");
  await app.waitFor("id:log-copy-entry");
  await app.press("id:log-copy-entry");
  expect(app.clipboard()).toContain("Gateway rejected expired token");
  await app.waitFor("id:log-copy-entry", { state: "gone" });

  await app.press("id:logs-tab-history");
  await app.waitFor("id:logs-archives-list");
  expect(
    app.texts().some((t) => t.includes("hopenconnect-2026-11-23.log")),
  ).toBe(true);
  expect(
    app.texts().some((t) => t.includes("hopenconnect-2026-11-22.log")),
  ).toBe(true);

  await app.press("id:logs-tab-current");
  await app.waitFor("id:logs-clear");
  app.onDialog((dlg) => dlg.buttons[dlg.buttons.length - 1]);
  await app.press("id:logs-clear");
  await app.waitUntil(() =>
    host.commands.some((c) => c.command === "logs.clear"),
  );
});

test("Settings preferences, About version copy, Privacy sub-page and zero-VPN audit (zh-CN & en)", async () => {
  for (const lang of ["zh-CN", "en"] as const) {
    const host = new MockHOpenHost({
      language: lang,
      systemLocale: lang === "zh-CN" ? "zh-CN" : "en-US",
    });
    const app = await open("../App.tsx", {
      platform: "ios",
      width: 390,
      height: 844,
      locale: lang === "zh-CN" ? "zh-CN" : "en-US",
      host: host.hostOptions(),
    });
    host.attachApp(app);

    await app.waitFor("id:home-status-card");
    assertNoVpnCopy(app.texts(), `${lang}/home`);

    await app.press("id:nav-profiles");
    await app.waitFor("id:profiles-list");
    assertNoVpnCopy(app.texts(), `${lang}/profiles`);

    await app.press("id:nav-statistics");
    await app.waitFor("id:stats-traffic-card");
    assertNoVpnCopy(app.texts(), `${lang}/statistics`);

    await app.press("id:nav-settings");
    await app.waitFor("id:settings-open-about");
    expect(app.shows("id:settings-open-theme")).toBe(true);
    expect(app.shows("id:settings-open-language")).toBe(true);
    expect(app.shows("id:pref-theme-dark")).toBe(false);
    expect(app.shows("id:pref-language-en")).toBe(false);
    assertNoVpnCopy(app.texts(), `${lang}/settings`);

    await app.press("id:settings-open-theme");
    await app.waitFor("id:pref-theme-dark");
    assertNoVpnCopy(app.texts(), `${lang}/theme`);
    await app.press("id:pref-theme-dark");
    await app.waitUntil(() =>
      host.commands.some(
        (c) => c.command === "preferences.set" && c.payload.theme === "dark",
      ),
    );
    await app.press("id:nav-back");
    await app.waitFor("id:settings-open-language");

    await app.press("id:settings-open-language");
    await app.waitFor("id:pref-language-system");
    assertNoVpnCopy(app.texts(), `${lang}/language`);
    await app.press("id:pref-language-system");
    await app.waitUntil(() =>
      host.commands.some(
        (c) =>
          c.command === "preferences.set" && c.payload.language === "system",
      ),
    );
    await app.press("id:nav-back");
    await app.waitFor("id:settings-open-about");

    await app.press("id:settings-open-logs");
    await app.waitFor("id:logs-list");
    assertNoVpnCopy(app.texts(), `${lang}/logs`);
    await app.press("id:nav-back");
    await app.waitFor("id:settings-open-about");

    await app.press("id:settings-open-about");
    await app.waitFor("id:about-version-card");
    assertNoVpnCopy(app.texts(), `${lang}/about`);
    await app.press("id:about-copy-version");
    expect(app.clipboard()).toContain("2.0.0");
    await app.waitFor("id:dismiss-toast-1");
    await app.press("id:dismiss-toast-1");
    await app.waitFor("id:sonner-viewport", { state: "gone" });
    await app.press("id:about-link-repo");
    expect(host.openedUrls).toContain(
      "https://github.com/harmony-contrib/h-openconnect",
    );

    await app.press("id:about-open-privacy");
    await app.waitFor("id:privacy-scroll");
    assertNoVpnCopy(app.texts(), `${lang}/privacy`);
    await app.press("id:nav-back");
    await app.waitFor("id:about-version-card");
  }
});

test("Cold-start hydration never flashes empty home cards", async () => {
  const seededHost = new MockHOpenHost({ language: "zh-CN" });
  const seeded = await open("../App.tsx", {
    platform: "ios",
    width: 390,
    height: 844,
    locale: "zh-CN",
    host: seededHost.hostOptions({ seedInitialDomains: true }),
  });
  seededHost.attachApp(seeded);
  // Seeded launch renders the active profile immediately on frame 0 with no empty state.
  expect(seeded.shows("id:home-status-card")).toBe(true);
  expect(seeded.shows("id:home-profile-hq-gw")).toBe(true);
  expect(seeded.texts().some((t) => t.includes("暂无连接配置"))).toBe(false);
  expect(seeded.texts().some((t) => t.includes("未选择配置"))).toBe(false);
});

test("Dismissing a home connection error resets connection state", async () => {
  const host = new MockHOpenHost({ language: "zh-CN" });
  host.lifecycle = "failed";
  host.lastError = "Authentication failed: invalid passcode";
  const app = await open("../App.tsx", {
    platform: "ios",
    width: 390,
    height: 844,
    locale: "zh-CN",
    host: host.hostOptions({ seedInitialDomains: true }),
  });
  host.attachApp(app);

  await app.waitFor("id:home-error-alert");
  expect(app.shows("id:home-error-dismiss")).toBe(true);
  assertNoVpnCopy(app.texts(), "zh-CN/home-error");

  await app.press("id:home-error-dismiss");
  await app.waitUntil(() =>
    host.commands.some((c) => c.command === "vpn.resetState"),
  );
  await app.waitFor("id:home-error-alert", { state: "gone" });
  expect(app.texts().some((t) => t.includes("未连接"))).toBe(true);
});
