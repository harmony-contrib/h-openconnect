import {
  announce,
  cleanup,
  computed,
  effect,
  fontScale,
  keymap,
  platform,
  reduceMotion,
  safeArea,
  setColorScheme,
  signal,
  Text,
  untracked,
  View,
  windowSize,
  type JSX,
} from "nsh";
import { titleBar } from "nsh/desktop";
import { BackHandler } from "nsh/mobile";
import { NavPage, NavStack, Tab, Tabs as BaseTabs } from "nsh/base";
import type { CoreClient } from "./client";
import {
  DesktopNavigation,
  desktopShortcuts,
  isSettingsSection,
  type SettingsSection,
} from "./desktop-navigation";
import { workspaceLayout, workspaceLayoutEquals } from "./layout";
import { isLifecycleConnected, type RuntimeDomain } from "./model";
import { AuthChallengeOverlay } from "./pages/auth-challenge";
import { HomePage } from "./pages/home";
import { LogsPage } from "./pages/logs";
import { ProfileEditorPage } from "./pages/profile-editor";
import { ProfilesPage } from "./pages/profiles";
import {
  AboutPage,
  LanguageSettingsPage,
  PrivacyPage,
  SettingsPage,
  ThemeSettingsPage,
} from "./pages/settings";
import { StatisticsPage } from "./pages/statistics";
import { Action, IconButton } from "./ui/button";
import {
  App,
  confirmAction,
  makeTranslator,
  RouteLayout,
  RouteOwner,
  useLifetime,
  type AppContext,
  type Page,
} from "./ui/context";
import { Icon, type IconName } from "./ui/icons";
import { feedbackStyle } from "./ui/motion";
import {
  NotificationCenter,
  Notifications,
  NotificationViewport,
} from "./ui/notifications";
import { Badge, Loading } from "./ui/surface";
import { Tabs } from "./ui/tabs";
import { chrome, density, focusRing, palette, spacing, typography } from "./ui/tokens";
import { Workspaces } from "./workspaces";

const phonePrimary: { page: Page; zh: string; en: string; icon: IconName }[] = [
  { page: "home", zh: "概览", en: "Overview", icon: "shield-check" },
  { page: "profiles", zh: "配置", en: "Connections", icon: "server" },
  { page: "statistics", zh: "统计", en: "Statistics", icon: "activity" },
  { page: "settings", zh: "设置", en: "Settings", icon: "settings" },
];

const pageTitles: Record<Page, [string, string]> = {
  home: ["概览", "Overview"],
  profiles: ["配置", "Connections"],
  "profile-editor": ["编辑配置", "Connection"],
  statistics: ["统计", "Statistics"],
  logs: ["日志", "Logs"],
  settings: ["设置", "Settings"],
  theme: ["主题", "Theme"],
  language: ["语言", "Language"],
  about: ["关于", "About"],
  privacy: ["隐私说明", "Privacy"],
};

function pageDomains(page: Page): RuntimeDomain[] {
  switch (page) {
    case "home":
      return ["config", "status", "profiles", "statistics"];
    case "profiles":
    case "profile-editor":
      return ["config", "status", "profiles"];
    case "statistics":
      return ["config", "status", "statistics"];
    case "logs":
      return ["config", "status", "logs"];
    case "settings":
    case "about":
    case "privacy":
      return ["config", "status", "about"];
    default:
      return ["config", "status"];
  }
}

function pageMeasure(page: Page, wide: boolean): number {
  if (!wide) return 640;
  if (page === "home") return 960;
  if (page === "profiles" || page === "statistics") return 1120;
  if (page === "logs") return 1180;
  return platform.desktop ? 720 : 640;
}

function pageRequiredDomains(item: Route): RuntimeDomain[] {
  switch (item.name) {
    case "home":
    case "profiles":
      return ["status", "profiles"];
    case "profile-editor":
      return item.query ? ["profiles"] : [];
    case "statistics":
      return ["statistics"];
    case "logs":
      return ["logs"];
    case "about":
      return ["about"];
    default:
      return [];
  }
}

function PageData(props: {
  client: CoreClient;
  domains: RuntimeDomain[];
  t: (zh: string, en: string) => string;
  children?: JSX.Element;
}) {
  const ready = () =>
    props.domains.length === 0 || props.client.domains.has(props.domains);
  // Avoid flashing an empty placeholder or spinner while fast local domain
  // queries settle on first entry to an unseeded page.
  const showLoading = signal(false);
  effect(() => {
    if (ready()) {
      showLoading.set(false);
      return;
    }
    const timer = setTimeout(() => {
      showLoading.set(true);
    }, 120);
    cleanup(() => clearTimeout(timer));
  });
  return (
    <If
      when={ready()}
      else={
        <View style={{ flex: 1 }}>
          <If when={showLoading()}>
            <Loading text={props.t("正在加载…", "Loading…")} />
          </If>
        </View>
      }
    >
      {props.children}
    </If>
  );
}

type Route = { name: Page; query?: string };

export function HOpenConnectApp(props: { client: CoreClient }) {
  const notifications = new NotificationCenter();
  for (const [source, tone] of [
    [props.client.error, "danger"],
    [props.client.warning, "warning"],
    [props.client.notice, "info"],
  ] as const) {
    effect(() => {
      const message = source();
      if (!message) return;
      untracked(() => {
        notifications.publish(message, tone);
        if (tone !== "info") announce(message);
      });
    });
  }
  cleanup(() => notifications.dispose());

  // Sync light/dark/system theme directly into NSH's color scheme.
  effect(() => {
    const theme = props.client.preferences().theme;
    setColorScheme(theme === "system" ? "system" : theme);
  });

  const workspaces = new Workspaces<Route>("home", { name: "home" });
  const sidebarExpanded = signal<boolean | undefined>(undefined);
  const layout = computed(
    () =>
      workspaceLayout(
        windowSize().width - safeArea().left - safeArea().right,
        platform.desktop,
        sidebarExpanded(),
        fontScale(),
      ),
    { equals: workspaceLayoutEquals },
  );

  const overlays = signal(0);
  const alive = useLifetime();
  const leaveGuards = new Map<unknown, Set<() => boolean | Promise<boolean>>>();
  let leaving = false;

  const logRecording = signal(false);
  const logRecordingIntent = signal(0);

  const route = () => workspaces.route();
  const page = () => route().name;

  // Keep domain subscription strictly scoped to the active page.
  effect(() => {
    const activePage = page();
    untracked(() => {
      void props.client.setInterests(pageDomains(activePage));
    });
  });

  const { t, isZh } = makeTranslator(props.client);

  const requestNavigation = (action: () => void) => {
    if (overlays() > 0) return;
    const guards = [...(leaveGuards.get(route()) || [])];
    if (!guards.length) {
      action();
      return;
    }
    if (leaving) return;
    const current = route();
    leaving = true;
    void (async () => {
      for (const check of guards) {
        if (!(await check())) return false;
      }
      return true;
    })()
      .then((accepted) => {
        if (accepted && alive() && route() === current) action();
      })
      .catch((err) => {
        props.client.pushError(String(err));
      })
      .finally(() => {
        leaving = false;
      });
  };

  const switchWorkspace = (next: Page) => {
    if (workspaces.root === next && workspaces.depth() === 1) return;
    requestNavigation(() => {
      workspaces.switch(next, () => ({ name: next }));
    });
  };

  const switchSettingsSection = (next: SettingsSection) => {
    if (page() === next) return;
    requestNavigation(() => {
      if (workspaces.depth() > 1) workspaces.replace({ name: next });
      else workspaces.switch(next, () => ({ name: next }));
    });
  };

  const navigate = (next: Page, query?: string) => {
    requestNavigation(() => {
      if (
        !query &&
        (next === "home" ||
          next === "profiles" ||
          next === "statistics" ||
          (layout().sidebar && next === "logs") ||
          next === "settings")
      ) {
        workspaces.switch(next, () => ({ name: next }));
      } else {
        workspaces.push({ name: next, query });
      }
    });
  };

  const back = () => {
    requestNavigation(() => {
      if (workspaces.depth() > 1) {
        workspaces.back();
      } else if (
        page() === "about" ||
        page() === "privacy" ||
        page() === "theme" ||
        page() === "language"
      ) {
        workspaces.switch("settings", () => ({ name: "settings" }));
      } else if (page() !== "home") {
        workspaces.switch("home", () => ({ name: "home" }));
      }
    });
  };

  const openProfileEditor = (profileId?: string) => {
    requestNavigation(() => {
      workspaces.push({ name: "profile-editor", query: profileId });
    });
  };

  // Hardware/system back button handling.
  cleanup(
    BackHandler.on("press", () => {
      if (overlays() > 0) return false;
      if (workspaces.depth() > 1) {
        back();
        return true;
      }
      if (page() !== "home") {
        switchWorkspace("home");
        return true;
      }
      return false;
    }),
  );

  // Desktop keyboard shortcuts (aligned with Paws PC navigation & refresh).
  if (platform.desktop) {
    for (const shortcut of desktopShortcuts) {
      cleanup(
        keymap.bind(shortcut.key, () => {
          if (overlays() === 0) switchWorkspace(shortcut.page);
        }),
      );
    }
    cleanup(
      keymap.bind("mod+b", () => {
        if (overlays() === 0) sidebarExpanded.set(!layout().expanded);
      }),
    );
    cleanup(
      keymap.bind("mod+n", () => {
        if (overlays() === 0 && page() !== "profile-editor") {
          openProfileEditor();
        }
      }),
    );
    effect(() => {
      if (page() === "profile-editor") return;
      cleanup(
        keymap.bind("mod+r", () => {
          if (overlays() === 0) {
            props.client.domains.refresh(pageDomains(page()));
          }
        }),
      );
    });
  }

  const appContext: AppContext = {
    client: props.client,
    route,
    page,
    navigate,
    switchWorkspace,
    back,
    t,
    isZh,
    setOverlayDelta: (delta) =>
      overlays.set(Math.max(0, overlays() + delta)),
    registerLeaveGuard: (owner, guard) => {
      let set = leaveGuards.get(owner);
      if (!set) {
        set = new Set();
        leaveGuards.set(owner, set);
      }
      set.add(guard);
      return () => {
        set?.delete(guard);
        if (set && set.size === 0) leaveGuards.delete(owner);
      };
    },
    logRecording,
    logRecordingIntent,
    openProfileEditor,
  };

  const wide = () => layout().sidebar;
  const showBack = () => workspaces.depth() > 1;
  const showBottomBar = () => !wide() && workspaces.depth() === 1;
  const showSettingsBar = () => wide() && isSettingsSection(page());

  const headerTitle = () => {
    const current = route();
    if (wide() && isSettingsSection(current.name)) {
      return t("设置", "Settings");
    }
    if (current.name === "profile-editor") {
      return current.query
        ? t("编辑配置", "Edit connection")
        : t("新建连接", "New connection");
    }
    const pair = pageTitles[current.name] ?? ["H-OpenConnect", "H-OpenConnect"];
    return t(pair[0], pair[1]);
  };

  const confirmClearLogs = () => {
    void confirmAction(
      appContext,
      t("清空当前日志？", "Clear current logs?"),
      async () => {
        await props.client.run("logs.clear", {}, t("日志已清空", "Logs cleared"));
      },
      { actionLabel: t("清空", "Clear") },
    );
  };

  const scanAndImportProfile = async () => {
    if (
      props.client.isPending("platform.scanQr") ||
      props.client.isPending("profiles.importQr")
    ) {
      return;
    }
    const content = await props.client.scanQr();
    if (!alive() || !content) return;
    const imported = await props.client.run<{ id: string; name: string }>(
      "profiles.importQr",
      { content },
    );
    if (alive() && imported?.name) {
      props.client.pushNotice(
        t(`已导入配置：${imported.name}`, `Imported connection: ${imported.name}`),
      );
    }
  };

  const renderRouteBody = (item: Route): JSX.Element => {
    switch (item.name) {
      case "home":
        return <HomePage />;
      case "profiles":
        return <ProfilesPage />;
      case "profile-editor":
        return <ProfileEditorPage profileId={item.query} />;
      case "statistics":
        return <StatisticsPage />;
      case "logs":
        return <LogsPage />;
      case "settings":
        return <SettingsPage />;
      case "theme":
        return <ThemeSettingsPage />;
      case "language":
        return <LanguageSettingsPage />;
      case "about":
        return <AboutPage />;
      case "privacy":
        return <PrivacyPage />;
      default:
        return <HomePage />;
    }
  };

  const connected = () =>
    isLifecycleConnected(
      props.client.domains.data("status")?.lifecycle ?? "disconnected",
    );

  return (
    <App.Provider value={appContext}>
      <Notifications.Provider value={notifications}>
        <View
          testID="app-root"
          style={{
            flex: 1,
            flexDirection: "row",
            backgroundColor: palette().background,
            paddingTop: platform.desktop ? 0 : safeArea().top,
            paddingBottom: showBottomBar() ? 0 : safeArea().bottom,
            paddingLeft: safeArea().left,
            paddingRight: safeArea().right,
          }}
        >
          <If when={wide()}>
            <DesktopNavigation
              page={page()}
              version={props.client.domains.data("about")?.appVersion || "2.0.0"}
              statusLabel={
                connected() ? t("已连接", "Connected") : t("未连接", "Idle")
              }
              connected={connected()}
              navigate={switchWorkspace}
              t={t}
              expanded={layout().expanded}
              onToggle={() => sidebarExpanded.set(!layout().expanded)}
            />
          </If>

          <View style={{ flex: 1, minWidth: 0, backgroundColor: palette().background }}>
            {/* Top header / desktop toolbar */}
            <View
              desktop={platform.desktop ? { windowDrag: true } : undefined}
              style={{
                backgroundColor: palette().background,
                borderBottomWidth: 1,
                borderBottomColor: palette().line,
              }}
            >
              <View
                style={{
                  minHeight: Math.max(
                    wide()
                      ? Math.max(
                          chrome.toolbar,
                          platform.desktop ? titleBar().height : 0,
                        )
                      : chrome.header,
                    density.line(24) * Math.max(1, fontScale()) + spacing.md,
                  ),
                  paddingLeft:
                    (wide() ? layout().inset : spacing.lg) +
                    (!wide() && platform.desktop ? titleBar().left : 0),
                  paddingRight:
                    (wide() ? layout().inset : spacing.lg) +
                    (platform.desktop ? titleBar().right : 0),
                  flexDirection: "row",
                  alignItems: "center",
                  gap: spacing.sm,
                }}
              >
                <If when={showBack()}>
                  <View desktop={platform.desktop ? { windowDrag: false } : undefined}>
                    <IconButton
                      id="nav-back"
                      icon="arrow-left"
                      label={t("返回", "Back")}
                      onPress={back}
                    />
                  </View>
                </If>
                <Text
                  testID="page-title"
                  role="header"
                  style={{
                    flex: 1,
                    minWidth: 0,
                    numberOfLines: 1,
                    fontSize: platform.desktop ? typography.md : typography.lg,
                    lineHeight: density.line(24),
                    fontWeight: "600",
                    color: palette().text,
                  }}
                >
                  {headerTitle()}
                </Text>
                <View desktop={platform.desktop ? { windowDrag: false } : undefined}>
                  <If when={wide() && (page() === "home" || page() === "statistics")}>
                    <Action
                      id="header-refresh"
                      small
                      secondary
                      icon="refresh-cw"
                      label={t("刷新", "Refresh")}
                      onPress={() =>
                        props.client.domains.refresh(pageDomains(page()))
                      }
                    />
                  </If>
                  <If when={page() === "profiles"}>
                    <View
                      style={{
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 6,
                      }}
                    >
                      <If when={props.client.canScanQr}>
                        <Action
                          id="header-scan-qr"
                          secondary
                          small
                          icon="scan-line"
                          loading={
                            props.client.isPending("platform.scanQr") ||
                            props.client.isPending("profiles.importQr")
                          }
                          label={t("扫码", "Scan")}
                          onPress={() => void scanAndImportProfile()}
                        />
                      </If>
                      <Action
                        id="header-add-profile"
                        primary={!platform.desktop}
                        secondary={platform.desktop}
                        small
                        icon="plus"
                        label={
                          platform.desktop
                            ? t("新建连接…", "New connection…")
                            : t("新建", "New")
                        }
                        onPress={() => openProfileEditor()}
                      />
                    </View>
                  </If>
                  <If when={page() === "logs"}>
                    <View
                      style={{
                        flexDirection: "row",
                        alignItems: "center",
                        gap: 6,
                      }}
                    >
                      <If when={wide()}>
                        <Badge
                          tone={logRecording() ? "brand" : "neutral"}
                          label={
                            logRecording()
                              ? t("正在记录", "Recording")
                              : t("已暂停", "Paused")
                          }
                        />
                      </If>
                      <Action
                        id="logs-toggle-recording"
                        small
                        stop={logRecording() && !wide()}
                        secondary={!logRecording() || wide()}
                        icon={logRecording() ? "stop" : "play"}
                        loading={props.client.isPending("logs.setRecording")}
                        label={
                          logRecording()
                            ? t("停止记录", "Stop")
                            : t("开始记录", "Record")
                        }
                        onPress={() =>
                          logRecordingIntent.set(logRecordingIntent() + 1)
                        }
                      />
                      <If
                        when={wide()}
                        else={
                          <IconButton
                            id="logs-clear"
                            small
                            icon="trash-2"
                            label={t("清空当前日志", "Clear logs")}
                            disabled={
                              props.client.isPending("logs.clear") ||
                              (props.client.domains.data("logs")?.entries
                                .length ?? 0) === 0
                            }
                            onPress={confirmClearLogs}
                          />
                        }
                      >
                        <Action
                          id="logs-clear"
                          small
                          secondary
                          icon="trash-2"
                          label={t("清空…", "Clear…")}
                          disabled={
                            props.client.isPending("logs.clear") ||
                            (props.client.domains.data("logs")?.entries
                              .length ?? 0) === 0
                          }
                          onPress={confirmClearLogs}
                        />
                      </If>
                    </View>
                  </If>
                </View>
              </View>
            </View>

            {/* Unified wide-screen Settings section bar (aligned with Paws) */}
            <If when={showSettingsBar()}>
              <View
                testID="settings-section-bar"
                style={{
                  width: "100%",
                  maxWidth: pageMeasure("settings", true),
                  alignSelf: "center",
                  paddingHorizontal: layout().inset,
                  paddingTop: spacing.md,
                  paddingBottom: spacing.xs,
                }}
              >
                <Tabs
                  idPrefix="settings-section"
                  inline={platform.desktop}
                  value={page()}
                  options={[
                    { value: "settings", label: t("外观", "Appearance") },
                    { value: "about", label: t("关于", "About") },
                  ]}
                  onChange={(next) =>
                    switchSettingsSection(next as SettingsSection)
                  }
                />
              </View>
            </If>

            {/* Independent workspace stacks */}
            <View style={{ flex: 1, minHeight: 0, position: "relative" }}>
              <Each of={workspaces.entries()} key={(entry) => entry.key}>
                {(entry) => {
                  const activeWorkspace = () =>
                    workspaces.active() === entry().key;
                  return (
                    <View
                      accessibilityElementsHidden={!activeWorkspace()}
                      style={{
                        position: "absolute",
                        top: 0,
                        right: 0,
                        bottom: 0,
                        left: 0,
                        visibility: activeWorkspace() ? "visible" : "hidden",
                        pointerEvents: activeWorkspace() ? "auto" : "none",
                      }}
                    >
                      <NavStack state={entry().nav} style={{ flex: 1 }}>
                        {(routeItem, index) => {
                          const routeLayout = computed(
                            () =>
                              workspaceLayout(
                                windowSize().width -
                                  safeArea().left -
                                  safeArea().right,
                                platform.desktop,
                                sidebarExpanded(),
                                fontScale(),
                                pageMeasure(routeItem.name, layout().sidebar),
                              ),
                            { equals: workspaceLayoutEquals },
                          );
                          return (
                            <NavPage
                              headerVisible={false}
                              animation={
                                reduceMotion() ||
                                platform.desktop ||
                                index() === 0
                                  ? "none"
                                  : "default"
                              }
                              style={{
                                flex: 1,
                                alignItems: "center",
                                backgroundColor: palette().background,
                              }}
                            >
                              <RouteOwner.Provider value={routeItem}>
                                <RouteLayout.Provider value={routeLayout}>
                                  <View
                                    style={{
                                      flex: 1,
                                      width: "100%",
                                      maxWidth: pageMeasure(
                                        routeItem.name,
                                        layout().sidebar,
                                      ),
                                    }}
                                  >
                                    <PageData
                                      client={props.client}
                                      domains={pageRequiredDomains(routeItem)}
                                      t={t}
                                    >
                                      {renderRouteBody(routeItem)}
                                    </PageData>
                                  </View>
                                </RouteLayout.Provider>
                              </RouteOwner.Provider>
                            </NavPage>
                          );
                        }}
                      </NavStack>
                    </View>
                  );
                }}
              </Each>
            </View>

            {/* Phone bottom navigation bar */}
            <If when={showBottomBar()}>
              <View
                testID="bottom-navigation"
                style={{
                  borderTopWidth: 1,
                  borderTopColor: palette().line,
                  backgroundColor: palette().panel,
                  paddingBottom: safeArea().bottom,
                }}
              >
                <BaseTabs
                  accessibilityLabel={t("底部导航", "Bottom navigation")}
                  style={{
                    height: chrome.bottom,
                    flexDirection: "row",
                    alignItems: "center",
                    paddingHorizontal: spacing.sm,
                  }}
                >
                  <Each of={phonePrimary}>
                    {(item) => {
                      const active = () => workspaces.root === item().page;
                      return (
                        <Tab
                          testID={`nav-${item().page}`}
                          selected={active()}
                          accessibilityLabel={t(item().zh, item().en)}
                          focusVisibleStyle={focusRing(0)}
                          onPress={() => switchWorkspace(item().page)}
                          style={{
                            flex: 1,
                            height: "100%",
                            alignItems: "center",
                            justifyContent: "center",
                            gap: 3,
                            ...feedbackStyle(),
                          }}
                        >
                          <Icon
                            name={item().icon}
                            size={20}
                            strokeWidth={active() ? 2.1 : 1.85}
                            color={
                              active() ? palette().accent : palette().muted
                            }
                          />
                          <Text
                            style={{
                              fontSize: 11,
                              lineHeight: 14,
                              fontWeight: active() ? "600" : "500",
                              color: active()
                                ? palette().text
                                : palette().muted,
                            }}
                          >
                            {t(item().zh, item().en)}
                          </Text>
                        </Tab>
                      );
                    }}
                  </Each>
                </BaseTabs>
              </View>
            </If>
          </View>

          <AuthChallengeOverlay />
          <NotificationViewport dismissLabel={t("关闭通知", "Dismiss")} />
        </View>
      </Notifications.Provider>
    </App.Provider>
  );
}
