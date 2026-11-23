import { cleanup, effect, platform, Scroll, signal, Text, View } from "nsh";
import {
  formatBytes,
  formatDuration,
  sanitizeDisplayText,
} from "../locale";
import {
  isLifecycleConnected,
  type ConnectionLifecycle,
  type ConnectionProfile,
} from "../model";
import { Action } from "../ui/button";
import { Toggle } from "../ui/choice";
import { useApp, useLayout } from "../ui/context";
import { Icon } from "../ui/icons";
import { animateDisclosure } from "../ui/motion";
import {
  Alert,
  Badge,
  Body,
  Card,
  Detail,
  Empty,
  ListRow,
  ListSurface,
  SectionHeader,
  Separator,
  SpeedGrid,
} from "../ui/surface";
import {
  density,
  palette,
  radius,
  spacing,
  surfaceFinish,
  typography,
  type StatusTone,
} from "../ui/tokens";

function lifecycleMeta(
  lifecycle: ConnectionLifecycle,
  t: (zh: string, en: string) => string,
): { label: string; tone: StatusTone } {
  switch (lifecycle) {
    case "connected":
      return { label: t("已连接", "Connected"), tone: "success" };
    case "connecting":
      return { label: t("正在连接", "Connecting"), tone: "info" };
    case "authenticating":
      return { label: t("正在验证身份", "Authenticating"), tone: "info" };
    case "establishing":
      return { label: t("正在建立隧道", "Establishing"), tone: "info" };
    case "disconnecting":
      return { label: t("正在断开", "Disconnecting"), tone: "warning" };
    case "failed":
      return { label: t("连接失败", "Connection failed"), tone: "danger" };
    default:
      return { label: t("未连接", "Disconnected"), tone: "neutral" };
  }
}

export function HomePage() {
  const ui = useApp();
  const layout = useLayout();
  const connectHandoff = signal(false);
  const optimisticForceGlobal = signal<boolean | undefined>(undefined);
  const optimisticActiveId = signal<string | undefined>(undefined);
  let handoffTimer: ReturnType<typeof setTimeout> | undefined;
  let statusAtConnect: unknown = undefined;
  let forceGlobalInFlight = false;

  const clearConnectHandoff = () => {
    if (handoffTimer !== undefined) {
      clearTimeout(handoffTimer);
      handoffTimer = undefined;
    }
    connectHandoff.set(false);
  };
  cleanup(() => {
    if (handoffTimer !== undefined) clearTimeout(handoffTimer);
  });

  const status = () => ui.client.domains.data("status");
  const profilesData = () => ui.client.domains.data("profiles");
  const statsData = () => ui.client.domains.data("statistics");

  effect(() => {
    const current = status();
    if (connectHandoff() && current !== statusAtConnect) {
      clearConnectHandoff();
    }
  });

  const lifecycle = (): ConnectionLifecycle =>
    status()?.lifecycle ?? "disconnected";
  const connected = () => isLifecycleConnected(lifecycle());
  const disconnecting = () =>
    (lifecycle() === "disconnecting" &&
      !ui.client.isPending("vpn.resetState")) ||
    ui.client.isPending("vpn.disconnect");
  const connecting = () =>
    lifecycle() === "connecting" ||
    lifecycle() === "authenticating" ||
    lifecycle() === "establishing" ||
    connectHandoff() ||
    ui.client.isPending("vpn.connect") ||
    (!!status()?.operationPending && !connected() && !disconnecting());
  const busy = () => connecting() || disconnecting();
  const profiles = (): ConnectionProfile[] => profilesData()?.profiles ?? [];
  const activeId = () =>
    optimisticActiveId() ??
    profilesData()?.activeConnectionId ??
    ui.client.domains.data("config")?.activeConnectionId ??
    "";
  const activeProfile = () =>
    profiles().find((item) => item.id === activeId());
  const forceGlobalValue = () =>
    optimisticForceGlobal() ?? !!activeProfile()?.forceGlobal;

  const latestSample = () => {
    const history = statsData()?.trafficHistory ?? [];
    return history.length > 0 ? history[history.length - 1] : undefined;
  };

  const primaryAction = () => {
    if (connected()) {
      void ui.client.run("vpn.disconnect");
      return;
    }
    if (busy()) {
      return;
    }
    if (!activeProfile()) {
      ui.openProfileEditor();
      return;
    }
    statusAtConnect = status();
    connectHandoff.set(true);
    if (handoffTimer !== undefined) clearTimeout(handoffTimer);
    handoffTimer = setTimeout(clearConnectHandoff, 1500);
    void ui.client.run("vpn.connect").then((res) => {
      if (res === undefined) {
        clearConnectHandoff();
      }
    });
  };

  const primaryLabel = () => {
    if (connected()) return ui.t("断开连接", "Disconnect");
    if (disconnecting()) return ui.t("正在断开…", "Disconnecting…");
    if (lifecycle() === "authenticating") {
      return ui.t("正在验证身份…", "Authenticating…");
    }
    if (lifecycle() === "establishing") {
      return ui.t("正在建立隧道…", "Establishing…");
    }
    if (connecting()) return ui.t("正在连接…", "Connecting…");
    if (!activeProfile()) return ui.t("新建连接", "New connection");
    return ui.t("立即连接", "Connect");
  };

  const quickProfiles = () => {
    const all = profiles();
    const favs = all.filter((item) => item.favorite);
    return (favs.length > 0 ? favs : all).slice(0, 6);
  };

  const renderHeroCard = () => {
    const meta = () => lifecycleMeta(lifecycle(), ui.t);
    const active = () => activeProfile();
    const lastError = () => sanitizeDisplayText(status()?.lastError || "");
    return (
      <Card id="home-status-card">
        <View
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: spacing.md,
          }}
        >
          <View
            style={{
              width: 48,
              height: 48,
              borderRadius: radius.xl,
              borderCurve: "continuous",
              borderWidth: 1,
              alignItems: "center",
              justifyContent: "center",
              ...(connected()
                ? surfaceFinish("accent")
                : surfaceFinish("raised")),
            }}
          >
            <Icon
              name="concentric-portal"
              size={28}
              color={connected() ? palette().primaryText : palette().accent}
            />
          </View>
          <View style={{ flex: 1, minWidth: 0, gap: 4 }}>
            <View
              style={{
                flexDirection: "row",
                alignItems: "center",
                gap: spacing.sm,
                flexWrap: "wrap",
              }}
            >
              <Text
                testID="home-profile-title"
                style={{
                  flexShrink: 1,
                  minWidth: 0,
                  numberOfLines: 1,
                  fontSize: typography.lg,
                  lineHeight: density.line(24),
                  fontWeight: "600",
                  color: palette().text,
                }}
              >
                {active()?.name || ui.t("未选择配置", "No connection selected")}
              </Text>
              <Badge
                id="home-lifecycle-badge"
                tone={meta().tone}
                label={meta().label}
              />
            </View>
            <Text
              style={{
                numberOfLines: 1,
                fontSize: typography.note,
                lineHeight: density.line(18),
                color: palette().muted,
              }}
            >
              {active()?.server ||
                ui.t(
                  "添加一个服务器配置以开始连接",
                  "Add a server profile to get started",
                )}
            </Text>
          </View>
        </View>

        <Action
          id="home-connection-action"
          fullWidth
          primary={!connected() && !disconnecting()}
          stop={connected() || disconnecting()}
          icon={
            connected()
              ? "stop"
              : busy()
                ? undefined
                : active()
                  ? "power"
                  : "plus"
          }
          loading={busy()}
          label={primaryLabel()}
          onPress={primaryAction}
        />

        <If when={!!lastError() || lifecycle() === "failed"}>
          <Alert
            id="home-error-alert"
            danger
            title={
              lifecycle() === "failed"
                ? ui.t("连接失败", "Connection failed")
                : ui.t("连接异常", "Connection error")
            }
            text={
              lastError() ||
              ui.t(
                "连接未能建立，关闭后将自动重置连接状态。",
                "Connection could not be established. Dismiss to reset state.",
              )
            }
            dismissId="home-error-dismiss"
            dismissLabel={ui.t(
              "关闭错误提示并重置连接状态",
              "Dismiss error and reset connection",
            )}
            dismissLoading={ui.client.isPending("vpn.resetState")}
            onDismiss={() => {
              animateDisclosure();
              void ui.client.run("vpn.resetState");
            }}
          />
        </If>

        <If when={!!active()}>
          <Separator />
          <Toggle
            id="home-force-global"
            label={ui.t("全局路由", "Full tunnel")}
            description={ui.t(
              "让所有流量通过当前安全连接",
              "Route all traffic through this connection",
            )}
            value={forceGlobalValue()}
            onChange={(next) => {
              if (forceGlobalInFlight) return;
              forceGlobalInFlight = true;
              optimisticForceGlobal.set(next);
              void ui.client
                .run(
                  "profiles.setForceGlobal",
                  { forceGlobal: next },
                  connected()
                    ? ui.t(
                        "已更新路由策略，重新连接后生效",
                        "Routing updated; reconnect to apply",
                      )
                    : ui.t("已更新路由策略", "Routing updated"),
                )
                .finally(() => {
                  forceGlobalInFlight = false;
                  optimisticForceGlobal.set(undefined);
                });
            }}
          />
        </If>
      </Card>
    );
  };

  const renderSessionSummary = () => {
    const stats = () => statsData()?.stats;
    return (
      <View style={{ gap: spacing.section }}>
        <SpeedGrid
          upload={latestSample()?.txBytesPerSec}
          download={latestSample()?.rxBytesPerSec}
          connected={connected()}
          formatBytes={formatBytes}
        />
        <Card
          id="home-session-card"
          title={ui.t("当前会话", "Current session")}
          right={
            <Action
              id="home-open-stats"
              variant="link"
              small
              label={ui.t("查看统计", "Details")}
              onPress={() => ui.switchWorkspace("statistics")}
            />
          }
        >
          <Detail
            label={ui.t("连接时长", "Duration")}
            value={
              connected() ? formatDuration(stats()?.connectedSeconds ?? 0) : "—"
            }
          />
          <Separator />
          <Detail
            label={ui.t("分配地址", "Assigned IP")}
            value={connected() ? stats()?.assignedIp || "—" : "—"}
          />
          <Separator />
          <Detail
            label={ui.t("网关", "Gateway")}
            value={connected() ? stats()?.gateway || activeProfile()?.server || "—" : "—"}
          />
          <Separator />
          <Detail
            label={ui.t("累计流量", "Transferred")}
            value={
              connected()
                ? `${formatBytes(stats()?.bytesReceived ?? 0)} ↓ · ${formatBytes(stats()?.bytesSent ?? 0)} ↑`
                : "—"
            }
          />
        </Card>
      </View>
    );
  };

  const renderQuickProfiles = () => (
    <View style={{ gap: spacing.sm }}>
      <SectionHeader
        title={ui.t("连接配置", "Connections")}
        right={
          <Action
            id="home-add-profile"
            small
            secondary
            icon="plus"
            label={ui.t("新建", "New")}
            onPress={() => ui.openProfileEditor()}
          />
        }
      />
      <ListSurface id="home-profiles-list">
        <If
          when={quickProfiles().length > 0}
          else={
            <Empty
              icon="server"
              title={ui.t("暂无连接配置", "No connections yet")}
              text={ui.t(
                "添加服务器地址与认证方式后即可一键连接。",
                "Add a server address and sign-in method to connect.",
              )}
            >
              <Action
                id="home-empty-add"
                primary
                small
                icon="plus"
                label={ui.t("新建连接", "New connection")}
                onPress={() => ui.openProfileEditor()}
              />
            </Empty>
          }
        >
          <Each of={quickProfiles()} key={(item) => item.id}>
            {(item, index) => {
              const isSelected = () => item().id === activeId();
              return (
                <ListRow
                  id={`home-profile-${item().id}`}
                  icon={item().favorite ? "star-filled" : "server"}
                  title={item().name}
                  subtitle={item().server}
                  selected={isSelected()}
                  disabled={busy() || (connected() && !isSelected())}
                  divider={index() + 1 < quickProfiles().length}
                  badge={
                    <If when={isSelected()}>
                      <Badge
                        tone={connected() ? "success" : "brand"}
                        label={
                          connected()
                            ? ui.t("已连接", "Active")
                            : ui.t("当前", "Selected")
                        }
                      />
                    </If>
                  }
                  onPress={() => {
                    if (isSelected()) return;
                    const targetId = item().id;
                    optimisticActiveId.set(targetId);
                    void ui.client
                      .run("profiles.select", { id: targetId })
                      .finally(() => {
                        if (optimisticActiveId() === targetId) {
                          optimisticActiveId.set(undefined);
                        }
                      });
                  }}
                />
              );
            }}
          </Each>
        </If>
      </ListSurface>
    </View>
  );

  return (
    <Scroll
      testID="home-scroll"
      bounces={false}
      keyboardDismissMode="on-drag"
      keyboardShouldPersistTaps="handled"
      style={{ flex: 1 }}
    >
      <Body>
        <If
          when={layout().overview && connected()}
          else={
            <View style={{ gap: spacing.section }}>
              {renderHeroCard()}
              <If when={connected()}>{renderSessionSummary()}</If>
              {renderQuickProfiles()}
            </View>
          }
        >
          <View
            style={{
              flexDirection: "row",
              gap: layout().gap,
              alignItems: "flex-start",
            }}
          >
            <View
              style={{
                width: Math.max(320, layout().connection),
                minWidth: 0,
                gap: spacing.section,
              }}
            >
              {renderHeroCard()}
              {renderQuickProfiles()}
            </View>
            <View style={{ flex: 1, minWidth: 0, gap: spacing.section }}>
              {renderSessionSummary()}
            </View>
          </View>
        </If>
      </Body>
    </Scroll>
  );
}
