import { Scroll, Text, View } from "nsh";
import type { LanguagePreference, ThemePreference } from "../model";
import { Action, Actions, ExternalLink } from "../ui/button";
import { RadioGroup } from "../ui/choice";
import { copyText, useApp, useLayout } from "../ui/context";
import { Icon } from "../ui/icons";
import {
  Body,
  Card,
  Detail,
  Hint,
  ListRow,
  ListSurface,
  SectionHeader,
  Separator,
} from "../ui/surface";
import {
  density,
  palette,
  radius,
  spacing,
  surfaceFinish,
  typography,
} from "../ui/tokens";

const REPO_URL = "https://github.com/harmony-contrib/h-openconnect";
const OPENCONNECT_URL = "https://www.infradead.org/openconnect/";
const OPENCONNECT_LICENSE_URL =
  "https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html";
const NSH_URL = "https://github.com/longbridge/nsh";

export function SettingsPage() {
  const ui = useApp();
  const layout = useLayout();
  const prefs = () => ui.client.preferences();
  const about = () => ui.client.domains.data("about");

  const themeSummary = () => {
    switch (prefs().theme) {
      case "light":
        return ui.t("浅色", "Light");
      case "dark":
        return ui.t("深色", "Dark");
      default:
        return ui.t("跟随系统", "System");
    }
  };

  const languageSummary = () => {
    switch (prefs().language) {
      case "zh-CN":
        return "简体中文";
      case "en":
        return "English";
      default:
        return ui.t("跟随系统", "System");
    }
  };

  return (
    <Scroll
      testID="settings-scroll"
      bounces={false}
      style={{ flex: 1 }}
    >
      <Body>
        <View style={{ gap: spacing.sm }}>
          <SectionHeader title={ui.t("偏好", "Preferences")} />
          <ListSurface>
            <ListRow
              id="settings-open-theme"
              icon="palette"
              title={ui.t("主题", "Theme")}
              value={themeSummary()}
              chevron
              divider
              onPress={() => ui.navigate("theme")}
            />
            <ListRow
              id="settings-open-language"
              icon="globe"
              title={ui.t("语言", "Language")}
              value={languageSummary()}
              chevron
              onPress={() => ui.navigate("language")}
            />
          </ListSurface>
        </View>

        <If when={!layout().sidebar}>
          <View style={{ gap: spacing.sm }}>
            <SectionHeader title={ui.t("更多", "More")} />
            <ListSurface>
              <ListRow
                id="settings-open-logs"
                icon="file-text"
                title={ui.t("日志", "Logs")}
                chevron
                divider
                onPress={() => ui.navigate("logs")}
              />
              <ListRow
                id="settings-open-about"
                icon="info"
                title={ui.t("关于", "About")}
                value={about()?.appVersion || "1.0.1"}
                tabular
                chevron
                onPress={() => ui.navigate("about")}
              />
            </ListSurface>
          </View>
        </If>
      </Body>
    </Scroll>
  );
}

export function ThemeSettingsPage() {
  const ui = useApp();
  const prefs = () => ui.client.preferences();

  const setTheme = (theme: string) => {
    const previous = ui.client.preferences();
    ui.client.preferences.set({
      ...previous,
      theme: theme as ThemePreference,
    });
    void ui.client
      .run("preferences.set", {
        theme: theme as ThemePreference,
      })
      .then((res) => {
        if (!res) ui.client.preferences.set(previous);
      });
  };

  return (
    <Scroll
      testID="theme-settings-scroll"
      bounces={false}
      style={{ flex: 1 }}
    >
      <Body>
        <View style={{ gap: spacing.sm }}>
          <SectionHeader title={ui.t("外观主题", "Appearance")} />
          <ListSurface>
            <RadioGroup
              idPrefix="pref-theme"
              insetRow
              label={ui.t("外观主题", "Appearance")}
              value={prefs().theme}
              options={[
                { value: "system", label: ui.t("跟随系统", "System") },
                { value: "light", label: ui.t("浅色", "Light") },
                { value: "dark", label: ui.t("深色", "Dark") },
              ]}
              onChange={setTheme}
            />
          </ListSurface>
          <View style={{ paddingHorizontal: spacing.xxs }}>
            <Hint
              text={ui.t(
                "选择「跟随系统」时，界面会自动适配设备的浅色与深色模式。",
                "When set to System, the interface follows your device appearance automatically.",
              )}
            />
          </View>
        </View>
      </Body>
    </Scroll>
  );
}

export function LanguageSettingsPage() {
  const ui = useApp();
  const prefs = () => ui.client.preferences();

  const setLanguage = (language: string) => {
    const previous = ui.client.preferences();
    ui.client.preferences.set({
      ...previous,
      language: language as LanguagePreference,
    });
    void ui.client
      .run("preferences.set", {
        language: language as LanguagePreference,
      })
      .then((res) => {
        if (!res) ui.client.preferences.set(previous);
      });
  };

  return (
    <Scroll
      testID="language-settings-scroll"
      bounces={false}
      style={{ flex: 1 }}
    >
      <Body>
        <View style={{ gap: spacing.sm }}>
          <SectionHeader title={ui.t("显示语言", "Display language")} />
          <ListSurface>
            <RadioGroup
              idPrefix="pref-language"
              insetRow
              label={ui.t("显示语言", "Display language")}
              value={prefs().language}
              options={[
                { value: "system", label: ui.t("跟随系统", "System") },
                { value: "zh-CN", label: "简体中文" },
                { value: "en", label: "English" },
              ]}
              onChange={setLanguage}
            />
          </ListSurface>
          <View style={{ paddingHorizontal: spacing.xxs }}>
            <Hint
              text={ui.t(
                "切换语言后界面文案将立即更新，无需重启应用。",
                "Interface copy updates immediately without restarting the app.",
              )}
            />
          </View>
        </View>
      </Body>
    </Scroll>
  );
}

export function AboutPage() {
  const ui = useApp();
  const about = () => ui.client.domains.data("about");

  const versionCopyText = () => {
    const lines = [`H-OpenConnect ${about()?.appVersion || "1.0.1"}`];
    if (about()?.openconnectVersion) {
      lines.push(`OpenConnect ${about()!.openconnectVersion}`);
    }
    return lines.join("\n");
  };

  return (
    <Scroll testID="about-scroll" bounces={false} style={{ flex: 1 }}>
      <Body>
        <View
          style={{
            alignItems: "center",
            gap: density.space(8),
            paddingTop: density.space(8),
            paddingBottom: density.space(12),
          }}
        >
          <View
            style={{
              width: 56,
              height: 56,
              borderWidth: 1,
              borderRadius: radius.xl,
              borderCurve: "continuous",
              alignItems: "center",
              justifyContent: "center",
              ...surfaceFinish("raised"),
            }}
          >
            <Icon
              name="concentric-portal"
              size={34}
              color={palette().accent}
            />
          </View>
          <Text
            style={{
              fontSize: density.font(20),
              fontWeight: "600",
              color: palette().text,
            }}
          >
            H-OpenConnect
          </Text>
          <Hint
            text={ui.t(
              "HarmonyOS 原生 AnyConnect 客户端",
              "Native AnyConnect client for HarmonyOS",
            )}
          />
        </View>

        <Card id="about-version-card" title={ui.t("版本", "Version")}>
          <Detail
            label={ui.t("应用版本", "App version")}
            value={about()?.appVersion || "1.0.1"}
          />
          <If when={!!about()?.openconnectVersion}>
            <Separator />
            <Detail
              label={ui.t("协议核心", "Protocol core")}
              value={about()?.openconnectVersion || "—"}
            />
          </If>
          <Actions align="start">
            <Action
              id="about-copy-version"
              secondary
              small
              icon="copy"
              label={ui.t("复制版本信息", "Copy version info")}
              onPress={() =>
                copyText(
                  ui,
                  versionCopyText(),
                  ui.t("版本信息已复制", "Version info copied"),
                )
              }
            />
          </Actions>
        </Card>

        <Card
          id="about-oss-card"
          title={ui.t("开源致谢", "Open source acknowledgements")}
        >
          <View style={{ gap: density.space(6) }}>
            <Text
              style={{
                fontSize: typography.sm,
                lineHeight: density.line(20),
                fontWeight: "500",
                color: palette().text,
              }}
            >
              H-OpenConnect
            </Text>
            <Actions align="start">
              <ExternalLink
                id="about-link-repo"
                small
                label={ui.t("源代码", "Source code")}
                href={REPO_URL}
              />
              <ExternalLink
                id="about-link-license"
                small
                label="MIT / Apache-2.0"
                href={`${REPO_URL}/blob/main/LICENSE`}
              />
            </Actions>
          </View>
          <Separator />
          <View style={{ gap: density.space(6) }}>
            <Text
              style={{
                fontSize: typography.sm,
                lineHeight: density.line(20),
                fontWeight: "500",
                color: palette().text,
              }}
            >
              OpenConnect
            </Text>
            <Actions align="start">
              <ExternalLink
                id="about-link-openconnect"
                small
                label={ui.t("项目主页", "Homepage")}
                href={OPENCONNECT_URL}
              />
              <ExternalLink
                id="about-link-openconnect-license"
                small
                label="LGPL-2.1"
                href={OPENCONNECT_LICENSE_URL}
              />
            </Actions>
          </View>
          <Separator />
          <View style={{ gap: density.space(6) }}>
            <Text
              style={{
                fontSize: typography.sm,
                lineHeight: density.line(20),
                fontWeight: "500",
                color: palette().text,
              }}
            >
              Native Shell
            </Text>
            <Actions align="start">
              <ExternalLink
                id="about-link-nsh"
                small
                label={ui.t("源代码", "Source code")}
                href={NSH_URL}
              />
            </Actions>
          </View>
        </Card>

        <Card id="about-privacy-card" title={ui.t("隐私与数据", "Privacy and data")}>
          <Action
            id="about-open-privacy"
            secondary
            icon="shield-check"
            label={ui.t("隐私说明", "Privacy notice")}
            onPress={() => ui.navigate("privacy")}
          />
        </Card>
      </Body>
    </Scroll>
  );
}

export function PrivacyPage() {
  const ui = useApp();
  const points = () => [
    ui.t(
      "所有服务器配置、证书文件与偏好设置仅保存在本机，不会上传至任何第三方服务器。",
      "All server profiles, certificates, and preferences stay on your device and are never uploaded to third-party servers.",
    ),
    ui.t(
      "发起连接时，应用仅直接与您配置的服务器及企业单点登录页面通信。",
      "When connecting, the app communicates only with the server and sign-in portal you configure.",
    ),
    ui.t(
      "日志记录默认关闭；开启后仅按日保存在本机，您可随时导出或删除历史归档。",
      "Log recording is off by default. When enabled, daily logs stay on your device and can be exported or deleted at any time.",
    ),
    ui.t(
      "应用不包含广告、统计追踪或遥测上报。",
      "This app contains no ads, analytics tracking, or telemetry.",
    ),
  ];

  return (
    <Scroll testID="privacy-scroll" bounces={false} style={{ flex: 1 }}>
      <Body>
        <Card title={ui.t("隐私与数据处理", "Privacy and data handling")}>
          <Each of={points()}>
            {(line, idx) => (
              <View style={{ gap: spacing.sm }}>
                <Text
                  style={{
                    userSelect: "text",
                    fontSize: density.font(14),
                    lineHeight: density.line(22),
                    color: palette().text,
                  }}
                >
                  {line()}
                </Text>
                <If when={idx() + 1 < points().length}>
                  <Separator />
                </If>
              </View>
            )}
          </Each>
        </Card>
      </Body>
    </Scroll>
  );
}
