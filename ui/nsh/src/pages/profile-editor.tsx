import { platform, Scroll, signal, Text, View } from "nsh";
import { Button } from "nsh/base";
import { fileBasename } from "../locale";
import {
  newDraftProfile,
  normalizeProfile,
  type AuthFieldChoice,
  type AuthGroupDiscovery,
  type AuthMethod,
  type ConnectionProfile,
  type SoftwareToken,
  type SplitTunnelMode,
} from "../model";
import { Action, Actions } from "../ui/button";
import { Check } from "../ui/choice";
import { useApp, useDraftGuard, useLifetime } from "../ui/context";
import { Field } from "../ui/field";
import { Icon } from "../ui/icons";
import { Select } from "../ui/menu";
import { animateDisclosure, feedbackStyle } from "../ui/motion";
import { Body, Card, Hint, Separator, Spinner } from "../ui/surface";
import {
  control,
  density,
  focusRing,
  palette,
  radius,
  spacing,
  surfaceFinish,
} from "../ui/tokens";

function serverLooksReady(server: string): boolean {
  const s = server.trim();
  return (
    s.length >= 3 &&
    !/\s/.test(s) &&
    !s.endsWith(".") &&
    !s.endsWith("/")
  );
}

export function ProfileEditorPage(props: { profileId?: string }) {
  const ui = useApp();
  const alive = useLifetime();

  const existing = props.profileId
    ? ui.client.domains
        .data("profiles")
        ?.profiles.find((p) => p.id === props.profileId)
    : undefined;

  const initial = existing ? normalizeProfile(existing) : newDraftProfile();
  const initialJson = JSON.stringify(initial);

  const draft = signal<ConnectionProfile>(initial);
  const showAdvanced = signal(false);
  const groupChoices = signal<AuthFieldChoice[]>([]);
  const groupLoading = signal(false);
  const groupHint = signal("");
  const savedClean = signal(false);
  let discoverSeq = 0;

  const isDirty = () =>
    !savedClean() && JSON.stringify(draft()) !== initialJson;

  useDraftGuard(
    isDirty,
    () => {
      draft.set(initial);
    },
    () => ui.client.isPending("profiles.save"),
  );

  const patch = <K extends keyof ConnectionProfile>(
    key: K,
    value: ConnectionProfile[K],
  ) => {
    draft.update((prev) => ({ ...prev, [key]: value }));
  };

  const resetDiscoveredGroups = () => {
    ++discoverSeq;
    groupLoading.set(false);
    groupChoices.set([]);
    groupHint.set("");
  };

  const discoverGroups = async () => {
    if (groupLoading()) return;
    const snapshot = draft();
    if (!serverLooksReady(snapshot.server)) {
      ui.client.pushWarning(
        ui.t("请先填写有效的服务器地址", "Enter a valid server address first"),
      );
      return;
    }
    const seq = ++discoverSeq;
    groupLoading.set(true);
    groupHint.set("");
    const result = await ui.client.run<AuthGroupDiscovery>(
      "profiles.discoverGroups",
      { profile: snapshot },
    );
    if (!alive() || seq !== discoverSeq) return;
    groupLoading.set(false);
    if (!result) return;
    const seen = new Set<string>();
    const choices = (result.groups || []).filter((g) => {
      const key = g.name.trim();
      if (!key || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
    groupChoices.set(choices);
    if (choices.length === 0) {
      groupHint.set(
        ui.t(
          "该服务器未返回可选分组，留空将使用默认分组。",
          "No selectable groups returned; leave empty to use the server default.",
        ),
      );
      return;
    }
    const currentGroup = draft().group.trim();
    const matched = choices.find(
      (g) => g.name === currentGroup || g.label.trim() === currentGroup,
    );
    if (matched) {
      patch("group", matched.name);
      groupHint.set(
        ui.t(
          `已加载 ${choices.length} 个可选分组。`,
          `Loaded ${choices.length} available groups.`,
        ),
      );
    } else {
      const fallback =
        (result.selected
          ? choices.find((g) => g.name === result.selected)
          : undefined) || choices[0];
      if (fallback) {
        patch("group", fallback.name);
        groupHint.set(
          currentGroup
            ? ui.t(
                "原分组不可用，已切换为服务器默认分组。",
                "Previous group was unavailable; switched to server default.",
              )
            : ui.t(
                `已加载 ${choices.length} 个可选分组。`,
                `Loaded ${choices.length} available groups.`,
              ),
        );
      }
    }
  };

  const pickCert = async (
    kind: "certificate" | "private_key" | "ca_certificate",
    field:
      | "certificate"
      | "privateKey"
      | "secondaryCertificate"
      | "secondaryPrivateKey"
      | "caCertificate",
  ) => {
    const path = await ui.client.pickCertFile(kind);
    if (alive() && path) {
      patch(field, path);
    }
  };

  const save = async () => {
    const current = draft();
    if (!current.name.trim() || !current.server.trim()) {
      ui.client.pushError(
        ui.t("请填写配置名称和服务器地址", "Name and server address are required"),
      );
      return;
    }
    const res = await ui.client.run<{ id: string }>(
      "profiles.save",
      { profile: current },
      ui.t("配置已保存", "Connection saved"),
    );
    if (alive() && res) {
      savedClean.set(true);
      ui.back();
    }
  };

  const needsPassword = () =>
    draft().authMethod === "password" ||
    draft().authMethod === "passwordAndCertificate";
  const needsCert = () =>
    draft().authMethod === "certificate" ||
    draft().authMethod === "passwordAndCertificate";

  return (
    <Scroll
      testID="profile-editor-scroll"
      bounces={false}
      keyboardDismissMode="on-drag"
      keyboardShouldPersistTaps="handled"
      style={{ flex: 1 }}
    >
      <Body>
        <Card title={ui.t("基本信息", "Connection")}>
          <Field
            id="editor-name"
            label={ui.t("名称", "Name")}
            placeholder={ui.t("例如：公司总部", "e.g. Corporate HQ")}
            value={draft().name}
            onChange={(v) => patch("name", v)}
          />
          <Field
            id="editor-server"
            label={ui.t("服务器地址", "Server address")}
            placeholder="gateway.example.com"
            keyboardType="url"
            value={draft().server}
            onChange={(v) => {
              patch("server", v);
              resetDiscoveredGroups();
            }}
          />
          <Select
            id="editor-auth-method"
            label={ui.t("认证方式", "Sign-in method")}
            value={draft().authMethod}
            options={[
              { value: "password", label: ui.t("账号密码", "Password") },
              { value: "certificate", label: ui.t("客户端证书", "Certificate") },
              {
                value: "passwordAndCertificate",
                label: ui.t("密码与证书", "Password + Certificate"),
              },
              { value: "saml", label: ui.t("网页单点登录 (SAML)", "Web SSO (SAML)") },
            ]}
            onChange={(v) => {
              const method = v as AuthMethod;
              patch("authMethod", method);
              if (method === "saml") {
                patch("externalBrowserAuth", true);
              }
              resetDiscoveredGroups();
            }}
          />

          <If when={needsPassword()}>
            <Field
              id="editor-username"
              label={ui.t("用户名", "Username")}
              placeholder={ui.t("输入账号", "Enter username")}
              value={draft().username}
              onChange={(v) => patch("username", v)}
            />
            <Field
              id="editor-password"
              label={ui.t("密码", "Password")}
              placeholder={ui.t("输入密码", "Enter password")}
              password
              value={draft().password}
              onChange={(v) => patch("password", v)}
            />
          </If>

          <If when={needsCert()}>
            <View style={{ gap: spacing.sm }}>
              <Field
                id="editor-cert"
                label={ui.t("客户端证书", "Client certificate")}
                placeholder={ui.t("选择 PEM / P12 证书文件", "Choose PEM / P12 file")}
                value={fileBasename(draft().certificate)}
                readonly
                onChange={() => {}}
              />
              <Actions align="start">
                <Action
                  id="editor-pick-cert"
                  small
                  secondary
                  icon="folder"
                  label={ui.t("选择证书文件", "Choose certificate")}
                  onPress={() => void pickCert("certificate", "certificate")}
                />
                <If when={!!draft().certificate}>
                  <Action
                    small
                    secondary
                    label={ui.t("清除", "Clear")}
                    onPress={() => patch("certificate", "")}
                  />
                </If>
              </Actions>
            </View>
            <View style={{ gap: spacing.sm }}>
              <Field
                id="editor-key"
                label={ui.t("私钥文件（可选）", "Private key (optional)")}
                placeholder={ui.t("独立私钥文件（如未内嵌于证书中）", "Separate key file if needed")}
                value={fileBasename(draft().privateKey)}
                readonly
                onChange={() => {}}
              />
              <Actions align="start">
                <Action
                  id="editor-pick-key"
                  small
                  secondary
                  icon="key"
                  label={ui.t("选择私钥", "Choose key")}
                  onPress={() => void pickCert("private_key", "privateKey")}
                />
                <If when={!!draft().privateKey}>
                  <Action
                    small
                    secondary
                    label={ui.t("清除", "Clear")}
                    onPress={() => patch("privateKey", "")}
                  />
                </If>
              </Actions>
            </View>
            <Field
              id="editor-key-pass"
              label={ui.t("证书密码（可选）", "Certificate passphrase (optional)")}
              password
              value={draft().keyPassword}
              onChange={(v) => patch("keyPassword", v)}
            />
          </If>

          <View style={{ gap: spacing.xs }}>
            <Text
              style={{
                fontSize: density.font(13),
                fontWeight: "500",
                color: palette().text,
              }}
            >
              {groupChoices().length > 0
                ? ui.t("认证分组", "Group")
                : ui.t("认证分组（可选）", "Group (optional)")}
            </Text>
            <View
              style={{
                flexDirection: "row",
                alignItems: "center",
                gap: spacing.sm,
              }}
            >
              <View style={{ flex: 1, minWidth: 0 }}>
                <If
                  when={groupChoices().length > 0}
                  else={
                    <Field
                      id="editor-group"
                      label=""
                      accessibilityLabel={ui.t("认证分组（可选）", "Group (optional)")}
                      placeholder={ui.t(
                        "留空使用服务器默认分组",
                        "Leave empty for server default",
                      )}
                      value={draft().group}
                      onChange={(v) => patch("group", v)}
                    />
                  }
                >
                  <Select
                    id="editor-group-select"
                    label={ui.t("认证分组", "Group")}
                    hideLabel
                    value={draft().group}
                    loading={groupLoading()}
                    options={groupChoices().map((g) => ({
                      value: g.name,
                      label: g.label || g.name,
                    }))}
                    onChange={(v) => patch("group", v)}
                  />
                </If>
              </View>
              <Button
                testID="editor-refresh-groups"
                accessibilityLabel={ui.t("刷新认证分组", "Refresh authentication groups")}
                disabled={groupLoading() || !serverLooksReady(draft().server)}
                focusVisibleStyle={focusRing()}
                hoverStyle={
                  groupLoading() || !serverLooksReady(draft().server)
                    ? undefined
                    : surfaceFinish("hover")
                }
                activeStyle={
                  groupLoading() || !serverLooksReady(draft().server)
                    ? undefined
                    : surfaceFinish("pressed")
                }
                onPress={() => void discoverGroups()}
                style={{
                  width: control.field,
                  height: control.field,
                  flexShrink: 0,
                  borderRadius: radius.md,
                  borderCurve: "continuous",
                  borderWidth: 1,
                  alignItems: "center",
                  justifyContent: "center",
                  opacity:
                    groupLoading()
                      ? 1
                      : !serverLooksReady(draft().server)
                        ? 0.45
                        : 1,
                  ...surfaceFinish("raised"),
                  ...feedbackStyle(),
                }}
              >
                <If
                  when={groupLoading()}
                  else={
                    <Icon
                      name="refresh-cw"
                      size={platform.desktop ? 14 : 17}
                      color={palette().text}
                    />
                  }
                >
                  <Spinner size={platform.desktop ? 14 : 17} color={palette().text} />
                </If>
              </Button>
            </View>
            <Hint
              text={
                groupLoading()
                  ? ui.t("正在获取服务器分组…", "Fetching server groups…")
                  : groupHint() ||
                    ui.t(
                      "完成认证方式与账号信息后点击右侧刷新加载分组，留空则使用默认分组。",
                      "Fill sign-in details then tap refresh to load groups, or leave empty for default.",
                    )
              }
            />
          </View>

          <Separator />
          <Check
            id="editor-force-global"
            label={ui.t("全局路由", "Full tunnel")}
            description={ui.t(
              "忽略服务器下发的分流规则，转发全部流量",
              "Route all device traffic through this connection",
            )}
            value={draft().forceGlobal}
            onChange={(v) => patch("forceGlobal", v)}
          />
          <Check
            id="editor-connect-on-demand"
            label={ui.t("断开后自动重连", "Reconnect automatically")}
            value={draft().connectOnDemand}
            onChange={(v) => patch("connectOnDemand", v)}
          />
          <Check
            id="editor-favorite"
            label={ui.t("收藏此配置", "Favorite")}
            value={draft().favorite}
            onChange={(v) => patch("favorite", v)}
          />
        </Card>

        <Card
          title={ui.t("高级选项", "Advanced options")}
          right={
            <Action
              id="editor-toggle-advanced"
              small
              secondary
              label={
                showAdvanced()
                  ? ui.t("收起", "Hide")
                  : ui.t("展开", "Show")
              }
              onPress={() => {
                animateDisclosure();
                showAdvanced.set(!showAdvanced());
              }}
            />
          }
        >
          <If
            when={showAdvanced()}
            else={
              <Hint
                text={ui.t(
                  "包含证书校验、分流网段、动态令牌、MTU、代理与应用过滤设置。",
                  "Certificate trust, split routing, TOTP token, MTU, proxy, and app filters.",
                )}
              />
            }
          >
            <View style={{ gap: spacing.md }}>
              <Check
                id="editor-strict-trust"
                label={ui.t("严格验证服务器证书", "Strict certificate verification")}
                value={draft().strictCertificateTrust}
                onChange={(v) => patch("strictCertificateTrust", v)}
              />
              <Check
                id="editor-block-untrusted"
                label={ui.t("拒绝不受信任的服务器", "Block untrusted servers")}
                value={draft().blockUntrustedServers}
                onChange={(v) => patch("blockUntrustedServers", v)}
              />
              <Check
                id="editor-use-dtls"
                label={ui.t("启用 DTLS 加速", "Enable DTLS transport")}
                value={draft().useDtls}
                onChange={(v) => patch("useDtls", v)}
              />
              <Check
                id="editor-allow-lan"
                label={ui.t("允许局域网直连", "Allow local LAN access")}
                value={draft().allowLocalLan}
                onChange={(v) => patch("allowLocalLan", v)}
              />
              <Check
                id="editor-external-browser"
                label={ui.t("允许网页单点登录", "Allow web single sign-on")}
                value={draft().externalBrowserAuth}
                onChange={(v) => patch("externalBrowserAuth", v)}
              />
              <Check
                id="editor-require-pfs"
                label={ui.t("要求前向保密 (PFS)", "Require Perfect Forward Secrecy")}
                value={draft().requirePfs}
                onChange={(v) => patch("requirePfs", v)}
              />
              <Check
                id="editor-fips"
                label={ui.t("FIPS 模式", "FIPS mode")}
                value={draft().fipsMode}
                onChange={(v) => patch("fipsMode", v)}
              />
              <Check
                id="editor-legacy-crypto"
                label={ui.t("兼容旧版加密算法", "Allow legacy cryptography")}
                value={draft().allowInsecureCrypto}
                onChange={(v) => patch("allowInsecureCrypto", v)}
              />
              <Check
                id="editor-disable-xml-post"
                label={ui.t("禁用 XML POST", "Disable XML POST")}
                value={draft().disableXmlPost}
                onChange={(v) => patch("disableXmlPost", v)}
              />

              <Separator />

              <Select
                id="editor-split-mode"
                label={ui.t("分流模式", "Split-tunnel mode")}
                value={draft().splitTunnelMode}
                options={[
                  { value: "auto", label: ui.t("跟随服务器", "Automatic") },
                  {
                    value: "onVpnDns",
                    label: ui.t("自定义网段 + 隧道 DNS", "Custom routes + tunnel DNS"),
                  },
                  {
                    value: "onUplinkDns",
                    label: ui.t("自定义网段 + 本地 DNS", "Custom routes + local DNS"),
                  },
                ]}
                onChange={(v) => patch("splitTunnelMode", v as SplitTunnelMode)}
              />
              <Field
                id="editor-split-networks"
                label={ui.t("分流网段 (CIDR)", "Split-tunnel networks (CIDR)")}
                placeholder="10.0.0.0/8, 192.168.10.0/24"
                value={draft().splitTunnelNetworks}
                onChange={(v) => patch("splitTunnelNetworks", v)}
              />

              <Separator />

              <Select
                id="editor-token-mode"
                label={ui.t("动态令牌", "Software token")}
                value={draft().softwareToken}
                options={[
                  { value: "disabled", label: ui.t("关闭", "Disabled") },
                  { value: "totp", label: "TOTP" },
                ]}
                onChange={(v) => patch("softwareToken", v as SoftwareToken)}
              />
              <If when={draft().softwareToken === "totp"}>
                <Field
                  id="editor-token-secret"
                  label={ui.t("令牌密钥", "Token secret")}
                  password
                  value={draft().tokenString}
                  onChange={(v) => patch("tokenString", v)}
                />
              </If>

              <Separator />

              <View style={{ gap: spacing.sm }}>
                <Field
                  id="editor-ca-cert"
                  label={ui.t("自定义 CA 证书", "Custom CA certificate")}
                  value={fileBasename(draft().caCertificate)}
                  readonly
                  onChange={() => {}}
                />
                <Actions align="start">
                  <Action
                    id="editor-pick-ca"
                    small
                    secondary
                    icon="folder"
                    label={ui.t("选择 CA 证书", "Choose CA certificate")}
                    onPress={() => void pickCert("ca_certificate", "caCertificate")}
                  />
                  <If when={!!draft().caCertificate}>
                    <Action
                      small
                      secondary
                      label={ui.t("清除", "Clear")}
                      onPress={() => patch("caCertificate", "")}
                    />
                  </If>
                </Actions>
              </View>

              <Field
                id="editor-cert-hash"
                label={ui.t("服务器证书指纹", "Server certificate pin")}
                placeholder="pin-sha256:..."
                value={draft().serverCertHash}
                onChange={(v) => patch("serverCertHash", v)}
              />
              <Field
                id="editor-backup-servers"
                label={ui.t("备用服务器", "Backup servers")}
                placeholder="backup1.example.com, backup2.example.com"
                value={draft().backupServers}
                onChange={(v) => patch("backupServers", v)}
              />
              <Field
                id="editor-http-proxy"
                label={ui.t("HTTP 代理", "HTTP proxy")}
                placeholder="http://proxy.example.com:8080"
                keyboardType="url"
                value={draft().httpProxy}
                onChange={(v) => patch("httpProxy", v)}
              />
              <Field
                id="editor-sni"
                label={ui.t("TLS SNI", "TLS SNI")}
                value={draft().sni}
                onChange={(v) => patch("sni", v)}
              />
              <Field
                id="editor-mtu"
                label="MTU"
                numeric
                placeholder={ui.t("自动 (576–1500)", "Auto (576–1500)")}
                value={draft().mtu > 0 ? String(draft().mtu) : ""}
                onChange={(v) => {
                  const n = Number.parseInt(v.trim(), 10);
                  patch("mtu", Number.isFinite(n) && n > 0 ? n : 0);
                }}
              />
              <Field
                id="editor-dpd"
                label={ui.t("保活探测间隔（秒）", "Dead-peer detection (seconds)")}
                numeric
                placeholder={ui.t("默认", "Default")}
                value={draft().dpdSeconds > 0 ? String(draft().dpdSeconds) : ""}
                onChange={(v) => {
                  const n = Number.parseInt(v.trim(), 10);
                  patch("dpdSeconds", Number.isFinite(n) && n > 0 ? n : 0);
                }}
              />
              <Field
                id="editor-trusted-apps"
                label={ui.t("仅代理以下应用（包名）", "Only tunnel these apps")}
                placeholder="com.example.app"
                value={draft().trustedApplications}
                onChange={(v) => patch("trustedApplications", v)}
              />
              <Field
                id="editor-blocked-apps"
                label={ui.t("排除以下应用（包名）", "Exclude these apps")}
                placeholder="com.example.local"
                value={draft().blockedApplications}
                onChange={(v) => patch("blockedApplications", v)}
              />
              <Field
                id="editor-user-agent"
                label={ui.t("自定义 User-Agent", "Custom User-Agent")}
                value={draft().userAgent}
                onChange={(v) => patch("userAgent", v)}
              />
              <Field
                id="editor-client-version"
                label={ui.t("上报客户端版本", "Reported client version")}
                value={draft().clientVersion}
                onChange={(v) => patch("clientVersion", v)}
              />
              <Field
                id="editor-reported-os"
                label={ui.t("上报系统标识", "Reported platform")}
                value={draft().reportedOs}
                onChange={(v) => patch("reportedOs", v)}
              />
            </View>
          </If>
        </Card>

        <Actions>
          <Action
            id="editor-cancel"
            secondary
            label={ui.t("取消", "Cancel")}
            onPress={() => ui.back()}
          />
          <Action
            id="editor-save"
            primary
            icon="check"
            loading={ui.client.isPending("profiles.save")}
            label={ui.t("保存配置", "Save")}
            onPress={() => void save()}
          />
        </Actions>
      </Body>
    </Scroll>
  );
}
