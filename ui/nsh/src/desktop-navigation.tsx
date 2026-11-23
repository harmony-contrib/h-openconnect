import { fontScale, keymap, platform, Scroll, Text, View } from "nsh";
import { titleBar } from "nsh/desktop";
import { Button, Tab, Tabs as BaseTabs, Tooltip } from "nsh/base";
import { Icon, type IconName } from "./ui/icons";
import type { Page } from "./ui/context";
import { feedbackStyle } from "./ui/motion";
import {
  chrome,
  density,
  focusRing,
  palette,
  radius,
  spacing,
  surfaceFinish,
} from "./ui/tokens";

interface Destination {
  page: Page;
  zh: string;
  en: string;
  icon: IconName;
  keybinding: string;
  testID: string;
}

export const desktopPrimary: Destination[] = [
  {
    page: "home",
    zh: "概览",
    en: "Overview",
    icon: "shield-check",
    keybinding: "mod+1",
    testID: "nav-home",
  },
  {
    page: "profiles",
    zh: "配置",
    en: "Connections",
    icon: "server",
    keybinding: "mod+2",
    testID: "nav-profiles",
  },
  {
    page: "statistics",
    zh: "统计",
    en: "Statistics",
    icon: "activity",
    keybinding: "mod+3",
    testID: "nav-statistics",
  },
  {
    page: "logs",
    zh: "日志",
    en: "Logs",
    icon: "file-text",
    keybinding: "mod+4",
    testID: "nav-logs",
  },
];

export const desktopSettings: Destination = {
  page: "settings",
  zh: "设置",
  en: "Settings",
  icon: "settings",
  keybinding: "mod+,",
  testID: "nav-settings",
};

export const desktopShortcuts = [
  ...desktopPrimary,
  desktopSettings,
].map((item) => ({ key: item.keybinding, page: item.page }));

export type SettingsSection = "settings" | "about";

export function isSettingsSection(page: Page): page is SettingsSection {
  return page === "settings" || page === "about";
}

function normalizeDestination(page: Page): Page {
  if (page === "profile-editor") return "profiles";
  if (
    page === "theme" ||
    page === "language" ||
    page === "about" ||
    page === "privacy"
  ) {
    return "settings";
  }
  return page;
}

export function DesktopNavigation(props: {
  page: Page;
  version: string;
  statusLabel: string;
  connected: boolean;
  navigate: (page: Page) => void;
  t: (zh: string, en: string) => string;
  expanded: boolean;
  onToggle: () => void;
}) {
  const expanded = () => props.expanded;
  const selected = (page: Page) => normalizeDestination(props.page) === page;

  const Item = (item: { destination: Destination }) => (
    <Tab
      tooltip={
        !expanded() ? (
          <Tooltip
            style={{
              maxWidth: 260,
              paddingHorizontal: spacing.sm,
              paddingVertical: density.space(4),
              borderRadius: radius.sm,
              borderCurve: "continuous",
              backgroundColor: palette().popover,
              borderWidth: 1,
              borderColor: palette().border,
              flexDirection: "row",
              alignItems: "center",
              gap: spacing.sm,
            }}
          >
            <Text style={{ fontSize: density.font(12), color: palette().text }}>
              {props.t(item.destination.zh, item.destination.en)}
            </Text>
            <If when={platform.desktop}>
              <Text
                style={{
                  fontSize: density.font(11),
                  color: palette().muted,
                  fontVariant: ["tabular-nums"],
                }}
              >
                {keymap.label(item.destination.keybinding)}
              </Text>
            </If>
          </Tooltip>
        ) : undefined
      }
      testID={item.destination.testID}
      accessibilityLabel={props.t(item.destination.zh, item.destination.en)}
      selected={selected(item.destination.page)}
      onPress={() => props.navigate(item.destination.page)}
      focusVisibleStyle={focusRing()}
      hoverStyle={
        selected(item.destination.page)
          ? undefined
          : { backgroundColor: palette().hover }
      }
      activeStyle={{ backgroundColor: palette().soft }}
      style={{
        minHeight: platform.desktop
          ? expanded()
            ? Math.max(28, density.line(18) * Math.max(1, fontScale()) + 10)
            : 36
          : expanded()
            ? Math.max(36, density.line(20) * Math.max(1, fontScale()) + 12)
            : 42,
        paddingHorizontal: expanded() ? (platform.desktop ? 8 : spacing.md) : 0,
        flexDirection: "row",
        alignItems: "center",
        justifyContent: expanded() ? "flex-start" : "center",
        gap: platform.desktop ? 9 : 10,
        borderRadius: platform.desktop
          ? expanded()
            ? 6
            : 8
          : expanded()
            ? 9
            : 10,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: selected(item.destination.page)
          ? palette().border
          : "transparent",
        backgroundColor: selected(item.destination.page)
          ? palette().sidebarAccent
          : "transparent",
        ...feedbackStyle(),
      }}
    >
      <Icon
        name={item.destination.icon}
        size={platform.desktop && expanded() ? 16 : 18}
        strokeWidth={selected(item.destination.page) ? 2.1 : 1.85}
        color={
          selected(item.destination.page) ? palette().text : palette().muted
        }
      />
      <View
        accessibilityElementsHidden={!expanded()}
        style={{
          display: expanded() ? "flex" : "none",
          flex: 1,
          minWidth: 0,
        }}
      >
        <Text
          style={{
            color: selected(item.destination.page)
              ? palette().text
              : palette().muted,
            fontSize: density.font(platform.desktop ? 13 : 14),
            lineHeight: density.line(platform.desktop ? 18 : 20),
            numberOfLines: 1,
            fontWeight: selected(item.destination.page)
              ? platform.desktop
                ? "500"
                : "600"
              : platform.desktop
                ? "400"
                : "500",
          }}
        >
          {props.t(item.destination.zh, item.destination.en)}
        </Text>
      </View>
      <If when={platform.desktop}>
        <Text
          accessibilityElementsHidden={!expanded()}
          style={{
            display: expanded() ? "flex" : "none",
            color: palette().faint,
            fontSize: 11,
            fontVariant: ["tabular-nums"],
          }}
        >
          {keymap.label(item.destination.keybinding)}
        </Text>
      </If>
    </Tab>
  );

  return (
    <View
      testID="desktop-sidebar"
      style={{
        width: expanded() ? 232 : 64,
        backgroundColor: palette().sidebar,
        borderRightWidth: 1,
        borderRightColor: palette().sidebarBorder,
      }}
    >
      <View
        desktop={platform.desktop ? { windowDrag: true } : undefined}
        style={{
          minHeight: platform.desktop
            ? Math.max(chrome.toolbar, titleBar().height)
            : chrome.toolbar,
          paddingTop:
            platform.desktop && !expanded() && titleBar().left > 0
              ? titleBar().height
              : 0,
          paddingLeft:
            platform.desktop && expanded() && titleBar().left > 0
              ? titleBar().left + 8
              : 8,
          paddingRight: 8,
          justifyContent: "center",
        }}
      >
        <View desktop={platform.desktop ? { windowDrag: false } : undefined}>
          <Button
            testID="desktop-toggle-sidebar"
            tooltip={
              !expanded() ? (
                <Tooltip
                  style={{
                    maxWidth: 260,
                    paddingHorizontal: spacing.sm,
                    paddingVertical: density.space(4),
                    borderRadius: radius.sm,
                    borderCurve: "continuous",
                    backgroundColor: palette().popover,
                    borderWidth: 1,
                    borderColor: palette().border,
                    flexDirection: "row",
                    alignItems: "center",
                    gap: spacing.sm,
                  }}
                >
                  <Text style={{ fontSize: density.font(12), color: palette().text }}>
                    {props.t("展开导航栏", "Expand sidebar")}
                  </Text>
                  <If when={platform.desktop}>
                    <Text
                      style={{
                        fontSize: density.font(11),
                        color: palette().muted,
                        fontVariant: ["tabular-nums"],
                      }}
                    >
                      {keymap.label("mod+b")}
                    </Text>
                  </If>
                </Tooltip>
              ) : undefined
            }
            accessibilityLabel={
              expanded()
                ? props.t("收起导航栏", "Collapse sidebar")
                : props.t("展开导航栏", "Expand sidebar")
            }
            onPress={props.onToggle}
            focusVisibleStyle={focusRing()}
            hoverStyle={{ backgroundColor: palette().hover }}
            activeStyle={{ backgroundColor: palette().soft }}
            style={{
              minHeight: platform.desktop ? 34 : 40,
              paddingHorizontal: expanded() ? 8 : 0,
              flexDirection: "row",
              alignItems: "center",
              justifyContent: expanded() ? "flex-start" : "center",
              gap: 9,
              borderRadius: platform.desktop ? 6 : 9,
              borderCurve: "continuous",
              ...feedbackStyle(),
            }}
          >
            <View
              accessibilityElementsHidden={!expanded()}
              style={{
                display: expanded() ? "flex" : "none",
                width: 26,
                height: 26,
                borderRadius: 8,
                borderCurve: "continuous",
                borderWidth: 1,
                alignItems: "center",
                justifyContent: "center",
                ...surfaceFinish("raised"),
              }}
            >
              <Icon
                name="concentric-portal"
                size={17}
                color={palette().accent}
              />
            </View>
            <Text
              accessibilityElementsHidden={!expanded()}
              style={{
                display: expanded() ? "flex" : "none",
                flex: 1,
                numberOfLines: 1,
                color: palette().text,
                fontSize: 15,
                fontWeight: "600",
                letterSpacing: -0.16,
              }}
            >
              H-OpenConnect
            </Text>
            <Icon
              name="panel-left"
              size={expanded() ? 16 : 18}
              strokeWidth={expanded() ? 2 : 1.85}
              color={palette().muted}
            />
          </Button>
        </View>
      </View>

      <Scroll
        bounces={false}
        style={{ flex: 1 }}
        contentStyle={{
          flexGrow: 1,
          paddingHorizontal: 8,
          paddingTop: 6,
          paddingBottom: 10,
        }}
      >
        <BaseTabs
          accessibilityLabel={props.t("导航", "Navigation")}
          style={{ flexGrow: 1, gap: platform.desktop ? 10 : spacing.md }}
        >
          <View style={{ gap: 2 }}>
            <Each of={desktopPrimary}>
              {(destination) => <Item destination={destination()} />}
            </Each>
          </View>
          <View style={{ flex: 1, minHeight: 8 }} />
          <View style={{ gap: 6 }}>
            <View
              style={{
                height: 1,
                marginHorizontal: expanded() ? 4 : 8,
                backgroundColor: palette().sidebarBorder,
              }}
            />
            <Item destination={desktopSettings} />
          </View>
        </BaseTabs>
      </Scroll>

      <View
        accessibilityElementsHidden={!expanded()}
        style={{
          display: expanded() ? "flex" : "none",
          paddingHorizontal: 14,
          paddingVertical: 10,
          borderTopWidth: 1,
          borderTopColor: palette().sidebarBorder,
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
        }}
      >
        <View
          style={{
            flex: 1,
            minWidth: 0,
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
          }}
        >
          <View
            style={{
              width: 7,
              height: 7,
              borderRadius: radius.full,
              backgroundColor: props.connected
                ? palette().success
                : palette().faint,
            }}
          />
          <Text
            style={{
              flexShrink: 1,
              minWidth: 0,
              numberOfLines: 1,
              fontSize: 12,
              lineHeight: 16,
              color: palette().muted,
            }}
          >
            {props.statusLabel}
          </Text>
        </View>
        <If when={!!props.version}>
          <Text
            style={{
              flexShrink: 1,
              minWidth: 0,
              numberOfLines: 1,
              fontSize: 12,
              lineHeight: 16,
              color: palette().muted,
              fontVariant: ["tabular-nums"],
            }}
          >
            {props.version.startsWith("v")
              ? props.version
              : `v${props.version}`}
          </Text>
        </If>
      </View>
    </View>
  );
}
