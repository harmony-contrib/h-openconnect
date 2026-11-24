import { Scroll, Text, View } from "nsh";
import { formatBytes, formatDuration } from "../locale";
import { isLifecycleConnected } from "../model";
import { TrafficChart } from "../ui/chart";
import { useApp, useLayout } from "../ui/context";
import {
  Badge,
  Body,
  Card,
  Detail,
  Empty,
  Separator,
  SpeedGrid,
} from "../ui/surface";
import { density, palette, spacing, typography } from "../ui/tokens";

export function StatisticsPage() {
  const ui = useApp();
  const layout = useLayout();
  const statsData = () => ui.client.domains.data("statistics");
  const profilesData = () => ui.client.domains.data("profiles");
  const connected = () =>
    isLifecycleConnected(statsData()?.lifecycle ?? "disconnected");
  const stats = () => statsData()?.stats;
  const network = () => statsData()?.network;
  const history = () => statsData()?.trafficHistory ?? [];
  const latest = () =>
    history().length > 0 ? history()[history().length - 1] : undefined;
  const activeProfile = () => {
    const id =
      profilesData()?.activeConnectionId ??
      ui.client.domains.data("config")?.activeConnectionId ??
      "";
    return profilesData()?.profiles.find((item) => item.id === id);
  };
  const dtlsEnabled = () => activeProfile()?.useDtls !== false;

  const renderTrafficCard = () => (
    <Card
      id="stats-traffic-card"
      title={ui.t("实时速率", "Live throughput")}
      right={
        <If when={connected()}>
          <Text
            testID="stats-window-meta"
            style={{
              fontSize: typography.note,
              lineHeight: density.line(18),
              color: palette().muted,
              fontVariant: ["tabular-nums"],
            }}
          >
            {ui.t("最近 30 秒", "Last 30s")}
          </Text>
        </If>
      }
    >
      <TrafficChart
        samples={connected() ? history() : []}
        downloadLabel={ui.t("下载", "Download")}
        uploadLabel={ui.t("上传", "Upload")}
      />
      <Separator />
      <Detail
        label={ui.t("已接收", "Received")}
        value={
          connected()
            ? `${formatBytes(stats()?.bytesReceived ?? 0)} (${stats()?.packetsReceived ?? 0})`
            : "—"
        }
      />
      <Separator />
      <Detail
        label={ui.t("已发送", "Sent")}
        value={
          connected()
            ? `${formatBytes(stats()?.bytesSent ?? 0)} (${stats()?.packetsSent ?? 0})`
            : "—"
        }
      />
      <Separator />
      <Detail
        label={ui.t("连接时长", "Duration")}
        value={
          connected() ? formatDuration(stats()?.connectedSeconds ?? 0) : "—"
        }
      />
    </Card>
  );

  const renderNetworkCard = () => (
    <Card
      id="stats-network-card"
      title={ui.t("网络信息", "Network")}
      right={
        <If when={connected()}>
          <Badge
            id="stats-transport-badge"
            tone={dtlsEnabled() ? "success" : "neutral"}
            label={
              dtlsEnabled()
                ? ui.t("DTLS 已启用", "DTLS enabled")
                : "TLS / CSTP"
            }
          />
        </If>
      }
    >
      <If
        when={connected()}
        else={
          <Empty
            icon="globe"
            title={ui.t("未建立连接", "Not connected")}
            text={ui.t(
              "连接后将显示分配的 IP、DNS 与路由详情。",
              "Assigned IP, DNS, and routes appear while connected.",
            )}
          />
        }
      >
        <View style={{ gap: spacing.md }}>
          <Detail
            label="IPv4"
            value={
              network()?.address
                ? network()?.netmask
                  ? `${network()?.address} / ${network()?.netmask}`
                  : network()!.address!
                : stats()?.assignedIp || "—"
            }
          />
          <If when={!!network()?.addressV6}>
            <Separator />
            <Detail
              label="IPv6"
              value={network()?.addressV6 || "—"}
            />
          </If>
          <Separator />
          <Detail
            label={ui.t("网关", "Gateway")}
            value={network()?.gateway || stats()?.gateway || "—"}
          />
          <Separator />
          <Detail
            label="MTU"
            value={
              (network()?.mtu || stats()?.mtu || 0) > 0
                ? String(network()?.mtu || stats()?.mtu)
                : "—"
            }
          />
          <Separator />
          <Detail
            stacked={(network()?.dns.length ?? 0) > 1}
            label="DNS"
            value={
              network()?.dns.length ? network()!.dns.join(", ") : "—"
            }
          />
          <If when={!!network()?.domain || (network()?.splitDns.length ?? 0) > 0}>
            <Separator />
            <Detail
              stacked
              label={ui.t("搜索域", "Search domains")}
              value={
                [
                  ...(network()?.domain ? [network()!.domain!] : []),
                  ...(network()?.splitDns ?? []),
                ].join(", ") || "—"
              }
            />
          </If>
          <If when={(network()?.routes.length ?? 0) > 0}>
            <Separator />
            <Detail
              stacked
              label={ui.t("路由规则", "Routes")}
              value={network()!.routes.slice(0, 12).join(", ")}
            />
          </If>
        </View>
      </If>
    </Card>
  );

  return (
    <Scroll
      testID="statistics-scroll"
      bounces={false}
      style={{ flex: 1 }}
    >
      <Body>
        <If
          when={layout().overview}
          else={
            <View style={{ gap: spacing.section }}>
              <SpeedGrid
                upload={latest()?.txBytesPerSec}
                download={latest()?.rxBytesPerSec}
                connected={connected()}
                formatBytes={formatBytes}
              />
              {renderTrafficCard()}
              {renderNetworkCard()}
            </View>
          }
        >
          <View
            testID="statistics-wide-workspace"
            style={{
              flexDirection: "row",
              gap: layout().gap,
              alignItems: "flex-start",
            }}
          >
            <View style={{ flex: 1, minWidth: 0, gap: spacing.section }}>
              <SpeedGrid
                upload={latest()?.txBytesPerSec}
                download={latest()?.rxBytesPerSec}
                connected={connected()}
                formatBytes={formatBytes}
              />
              {renderTrafficCard()}
            </View>
            <View style={{ flex: 1, minWidth: 0, gap: spacing.section }}>
              {renderNetworkCard()}
            </View>
          </View>
        </If>
      </Body>
    </Scroll>
  );
}
