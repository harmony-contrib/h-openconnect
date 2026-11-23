import { batch, computed, Image, platform, Scroll, signal, Text, View, windowSize } from "nsh";
import {
  isLifecycleBusy,
  isLifecycleConnected,
  type AuthMethod,
  type ConnectionProfile,
} from "../model";
import { Action, Actions, IconButton } from "../ui/button";
import { confirmAction, useApp, useLayout, useLifetime } from "../ui/context";
import { SearchField } from "../ui/field";
import { ObjectMenu, type ObjectCommand } from "../ui/menu";
import { Dialog } from "../ui/overlay";
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
} from "../ui/surface";
import { Tabs } from "../ui/tabs";
import { density, palette, radius, spacing, typography } from "../ui/tokens";

function authMethodLabel(
  method: AuthMethod,
  t: (zh: string, en: string) => string,
): string {
  switch (method) {
    case "certificate":
      return t("客户端证书", "Certificate");
    case "passwordAndCertificate":
      return t("密码与证书", "Password + Cert");
    case "saml":
      return t("网页单点登录", "Web SSO");
    default:
      return t("账号密码", "Password");
  }
}

export function ProfilesPage() {
  const ui = useApp();
  const layout = useLayout();
  const alive = useLifetime();
  const query = signal("");
  const tab = signal("all");
  const inspectedId = signal<string>("");
  const qrTarget = signal<ConnectionProfile | undefined>(undefined);
  const qrImage = signal<{ source: string; pngBase64: string } | undefined>(
    undefined,
  );
  const qrLoading = signal(false);
  const optimisticActiveId = signal<string | undefined>(undefined);
  const optimisticFavorites = signal<Record<string, boolean>>({});
  let qrSeq = 0;

  const openQrExport = async (profile: ConnectionProfile) => {
    if (qrLoading()) return;
    const seq = ++qrSeq;
    qrLoading.set(true);
    try {
      const encoded = await ui.client.run<{ name: string; payload: string }>(
        "profiles.encodeQr",
        { id: profile.id },
      );
      if (!alive() || seq !== qrSeq || !encoded?.payload) return;
      const img = await ui.client.generateQr(encoded.payload);
      if (!alive() || seq !== qrSeq || !img) return;
      // Open the dialog only after the QR PNG is ready so the modal never
      // pops in as a small spinner and jumps height mid-animation.
      batch(() => {
        qrImage.set(img);
        qrTarget.set(profile);
      });
    } finally {
      if (alive() && seq === qrSeq) {
        qrLoading.set(false);
      }
    }
  };

  const status = () => ui.client.domains.data("status");
  const profilesData = () => ui.client.domains.data("profiles");
  const profiles = (): ConnectionProfile[] => profilesData()?.profiles ?? [];
  const activeId = () =>
    optimisticActiveId() ?? profilesData()?.activeConnectionId ?? "";
  const isFavorite = (item: ConnectionProfile) => {
    const override = optimisticFavorites()[item.id];
    return override !== undefined ? override : item.favorite;
  };
  const lifecycle = () => status()?.lifecycle ?? "disconnected";
  const liveOrBusy = () =>
    isLifecycleBusy(lifecycle()) || isLifecycleConnected(lifecycle());

  const filtered = computed(() => {
    const q = query().trim().toLowerCase();
    const favOnly = tab() === "favorites";
    return profiles().filter((item) => {
      if (favOnly && !item.favorite) return false;
      if (!q) return true;
      return (
        item.name.toLowerCase().includes(q) ||
        item.server.toLowerCase().includes(q) ||
        item.group.toLowerCase().includes(q) ||
        item.username.toLowerCase().includes(q)
      );
    });
  });

  const inspectedProfile = () => {
    const list = filtered();
    const chosen = list.find((p) => p.id === inspectedId());
    if (chosen) return chosen;
    const active = list.find((p) => p.id === activeId());
    return active ?? list[0];
  };

  const deleteProfile = (profile: ConnectionProfile) => {
    void confirmAction(
      ui,
      ui.t(`删除“${profile.name}”？`, `Delete “${profile.name}”?`),
      async () => {
        await ui.client.run(
          "profiles.delete",
          { id: profile.id },
          ui.t("配置已删除", "Connection deleted"),
        );
      },
      {
        description: ui.t(
          "将移除此服务器配置及不再引用的本地证书副本。",
          "This removes the profile and any unreferenced certificate copies.",
        ),
        actionLabel: ui.t("删除", "Delete"),
      },
    );
  };

  const selectProfile = (targetId: string) => {
    if (targetId === activeId() || liveOrBusy()) return;
    optimisticActiveId.set(targetId);
    void ui.client.run("profiles.select", { id: targetId }).finally(() => {
      if (optimisticActiveId() === targetId) {
        optimisticActiveId.set(undefined);
      }
    });
  };

  const toggleFavorite = (profile: ConnectionProfile) => {
    const id = profile.id;
    const next = !isFavorite(profile);
    optimisticFavorites.update((m) => ({
      ...m,
      [id]: next,
    }));
    void ui.client.run("profiles.toggleFavorite", { id }).finally(() => {
      optimisticFavorites.update((m) => {
        if (!(id in m)) return m;
        const copy = { ...m };
        delete copy[id];
        return copy;
      });
    });
  };

  const buildCommands = (profile: ConnectionProfile): ObjectCommand[] => {
    const isActive = profile.id === activeId();
    const favored = isFavorite(profile);
    const locked = isActive && liveOrBusy();
    const commands: ObjectCommand[] = [
      {
        id: `profile-select-${profile.id}`,
        icon: "check",
        label: ui.t("设为当前连接", "Set as active"),
        disabled: isActive || liveOrBusy(),
        disabledReason: liveOrBusy()
          ? ui.t("请先断开连接", "Disconnect first")
          : undefined,
        run: () => selectProfile(profile.id),
      },
      {
        id: `profile-edit-${profile.id}`,
        icon: "file-pen-line",
        label: ui.t("编辑配置", "Edit"),
        disabled: locked,
        disabledReason: locked
          ? ui.t("请先断开连接", "Disconnect first")
          : undefined,
        run: () => ui.openProfileEditor(profile.id),
      },
      {
        id: `profile-favorite-${profile.id}`,
        icon: favored ? "star-filled" : "star",
        label: favored
          ? ui.t("取消收藏", "Remove favorite")
          : ui.t("收藏", "Add to favorites"),
        run: () => toggleFavorite(profile),
      },
    ];
    if (ui.client.canGenerateQr) {
      commands.push({
        id: `profile-qr-${profile.id}`,
        icon: "qr-code",
        label: ui.t("导出二维码…", "Export QR…"),
        run: () => {
          void openQrExport(profile);
        },
      });
    }
    commands.push({
      id: `profile-delete-${profile.id}`,
      icon: "trash-2",
      label: ui.t("删除配置…", "Delete…"),
      danger: true,
      disabled: locked,
      disabledReason: locked
        ? ui.t("请先断开连接", "Disconnect first")
        : undefined,
      run: () => deleteProfile(profile),
    });
    return commands;
  };

  const renderListPane = () => (
    <View style={{ gap: spacing.md }}>
      <If when={profiles().length > 0}>
        <View
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: spacing.sm,
            flexWrap: "wrap",
          }}
        >
          <Tabs
            inline={platform.desktop}
            idPrefix="profiles-tab"
            value={tab()}
            options={[
              {
                value: "all",
                label: ui.t("全部", "All"),
                count: profiles().length,
              },
              {
                value: "favorites",
                label: ui.t("收藏", "Favorites"),
                count: profiles().filter((p) => isFavorite(p)).length,
              },
            ]}
            onChange={(next) => tab.set(next)}
          />
          <View style={{ flex: 1, minWidth: 160 }}>
            <SearchField
              id="profiles-search"
              label={ui.t("搜索配置", "Search connections")}
              placeholder={ui.t("搜索名称或服务器", "Search name or server")}
              value={query()}
              onChange={(value) => query.set(value)}
            />
          </View>
        </View>
      </If>

      <ListSurface id="profiles-list">
        <If
          when={filtered().length > 0}
          else={
            <Empty
              icon="server"
              title={
                query() || tab() === "favorites"
                  ? ui.t("没有匹配的配置", "No matching connections")
                  : ui.t("暂无连接配置", "No connections yet")
              }
              text={
                query() || tab() === "favorites"
                  ? ui.t("调整搜索词或切换分类查看。", "Try a different filter.")
                  : ui.t(
                      "新建一个服务器配置以开始安全连接。",
                      "Create a server profile to get started.",
                    )
              }
            >
              <If
                when={!query() && tab() === "all"}
                else={
                  <Action
                    small
                    secondary
                    label={ui.t("清除筛选", "Clear filters")}
                    onPress={() => {
                      query.set("");
                      tab.set("all");
                    }}
                  />
                }
              >
                <Action
                  id="profiles-empty-add"
                  primary
                  small
                  icon="plus"
                  label={ui.t("新建连接", "New connection")}
                  onPress={() => ui.openProfileEditor()}
                />
              </If>
            </Empty>
          }
        >
          <Each of={filtered()} key={(item) => item.id}>
            {(item, index) => {
              const isActive = () => item().id === activeId();
              const favored = () => isFavorite(item());
              const isInspected = () =>
                layout().masterDetail && inspectedProfile()?.id === item().id;
              return (
                <ObjectMenu
                  id={`profile-menu-${item().id}`}
                  title={item().name}
                  subtitle={item().server}
                  commands={buildCommands(item())}
                  row={(menuTrigger) => (
                    <ListRow
                      id={`profile-row-${item().id}`}
                      icon={favored() ? "star-filled" : "server"}
                      title={item().name}
                      subtitle={item().server}
                      selected={isInspected() || (!layout().masterDetail && isActive())}
                      divider={index() + 1 < filtered().length}
                      badge={
                        <If when={isActive()}>
                          <Badge
                            tone={isLifecycleConnected(lifecycle()) ? "success" : "brand"}
                            label={
                              isLifecycleConnected(lifecycle())
                                ? ui.t("已连接", "Connected")
                                : ui.t("当前", "Active")
                            }
                          />
                        </If>
                      }
                      trailing={
                        <View
                          style={{
                            flexDirection: "row",
                            alignItems: "center",
                            gap: 2,
                          }}
                        >
                          <IconButton
                            id={`profile-star-${item().id}`}
                            small
                            icon={favored() ? "star-filled" : "star"}
                            iconColor={
                              favored() ? palette().accent : palette().faint
                            }
                            label={
                              favored()
                                ? ui.t("取消收藏", "Unfavorite")
                                : ui.t("收藏", "Favorite")
                            }
                            onPress={() => toggleFavorite(item())}
                          />
                          {menuTrigger}
                        </View>
                      }
                      onPress={() => {
                        if (layout().masterDetail) {
                          inspectedId.set(item().id);
                        } else if (!liveOrBusy()) {
                          selectProfile(item().id);
                        } else {
                          ui.openProfileEditor(item().id);
                        }
                      }}
                    />
                  )}
                />
              );
            }}
          </Each>
        </If>
      </ListSurface>
    </View>
  );

  const renderDetailPane = () => {
    const profile = () => inspectedProfile();
    return (
      <If
        when={!!profile()}
        else={
          <ListSurface>
            <Empty
              icon="server"
              title={ui.t("未选择配置", "No connection selected")}
              text={ui.t(
                "在左侧选择任一配置查看详情或编辑。",
                "Select a connection on the left to view or edit.",
              )}
            />
          </ListSurface>
        }
      >
        {(() => {
          const item = () => profile()!;
          const isActive = () => item().id === activeId();
          const locked = () => isActive() && liveOrBusy();
          return (
            <Card
              id="profile-detail-card"
              title={item().name}
              subtitle={item().server}
              right={
                <If when={isActive()}>
                  <Badge
                    tone={isLifecycleConnected(lifecycle()) ? "success" : "brand"}
                    label={
                      isLifecycleConnected(lifecycle())
                        ? ui.t("已连接", "Connected")
                        : ui.t("当前配置", "Active")
                    }
                  />
                </If>
              }
            >
              <Detail
                label={ui.t("认证方式", "Sign-in method")}
                value={authMethodLabel(item().authMethod, ui.t)}
              />
              <Separator />
              <Detail
                label={ui.t("认证分组", "Group")}
                value={item().group || ui.t("默认", "Default")}
              />
              <Separator />
              <Detail
                label={ui.t("用户名", "Username")}
                value={item().username || "—"}
              />
              <Separator />
              <Detail
                label={ui.t("路由策略", "Routing")}
                value={
                  item().forceGlobal
                    ? ui.t("全局路由", "Full tunnel")
                    : ui.t("自动分流", "Auto split")
                }
              />
              <Separator />
              <Detail
                label={ui.t("自动重连", "Auto reconnect")}
                value={
                  item().connectOnDemand
                    ? ui.t("已开启", "Enabled")
                    : ui.t("已关闭", "Off")
                }
              />
              <Separator />
              <Actions align="start">
                <If when={!isActive()}>
                  <Action
                    id="detail-select-profile"
                    primary
                    small
                    icon="check"
                    label={ui.t("设为当前", "Set active")}
                    disabled={liveOrBusy()}
                    onPress={() => selectProfile(item().id)}
                  />
                </If>
                <Action
                  id="detail-edit-profile"
                  secondary
                  small
                  icon="file-pen-line"
                  label={ui.t("编辑", "Edit")}
                  disabled={locked()}
                  onPress={() => ui.openProfileEditor(item().id)}
                />
                <If when={ui.client.canGenerateQr}>
                  <Action
                    id="detail-qr-profile"
                    secondary
                    small
                    icon="qr-code"
                    loading={qrLoading()}
                    label={ui.t("二维码", "QR")}
                    onPress={() => void openQrExport(item())}
                  />
                </If>
                <Action
                  id="detail-delete-profile"
                  danger
                  small
                  icon="trash-2"
                  label={ui.t("删除", "Delete")}
                  disabled={locked()}
                  onPress={() => deleteProfile(item())}
                />
              </Actions>
            </Card>
          );
        })()}
      </If>
    );
  };

  return (
    <>
      <Scroll
        testID="profiles-scroll"
        bounces={false}
        keyboardDismissMode="on-drag"
        keyboardShouldPersistTaps="handled"
        style={{ flex: 1 }}
      >
        <Body>
          <If when={layout().masterDetail} else={renderListPane()}>
            <View
              testID="profiles-master-detail"
              style={{
                flexDirection: "row",
                gap: layout().gap,
                alignItems: "flex-start",
              }}
            >
              <View
                style={{
                  width: layout().collection,
                  flexShrink: 0,
                  minWidth: 0,
                }}
              >
                {renderListPane()}
              </View>
              <View style={{ flex: 1, minWidth: layout().detailMin }}>
                {renderDetailPane()}
              </View>
            </View>
          </If>
        </Body>
      </Scroll>

      <If when={!!qrTarget() && !!qrImage()}>
        <Dialog
          width={platform.desktop ? 400 : 440}
          title={ui.t("连接二维码", "Connection QR code")}
          description={qrTarget()!.name}
          onClose={() =>
            batch(() => {
              qrTarget.set(undefined);
              qrImage.set(undefined);
            })
          }
        >
          <View style={{ gap: density.space(14), alignItems: "center" }}>
            <View
              style={{
                padding: 12,
                borderRadius: radius.lg,
                borderCurve: "continuous",
                backgroundColor: "#ffffff",
                borderWidth: 1,
                borderColor: palette().border,
              }}
            >
              <Image
                testID="profile-qr-image"
                src={qrImage()!.source}
                style={{
                  width: Math.max(
                    160,
                    Math.min(220, windowSize().width - 104),
                  ),
                  height: Math.max(
                    160,
                    Math.min(220, windowSize().width - 104),
                  ),
                }}
              />
            </View>
            <Alert
              warning
              text={ui.t(
                "此二维码包含已保存的凭据，请仅分享给可信对象。",
                "This QR code contains saved credentials. Share only with trusted recipients.",
              )}
            />
            <Actions>
              <Action
                primary
                id="export-profile-qr"
                icon="download"
                loading={ui.client.isPending("platform.exportQr")}
                label={ui.t("保存二维码图片", "Save QR image")}
                onPress={() => {
                  const target = qrTarget();
                  const img = qrImage();
                  if (!target || !img) return;
                  void ui.client.exportQr(
                    target.name,
                    img.pngBase64,
                    ui.t("二维码图片已保存", "QR image saved"),
                  );
                }}
              />
            </Actions>
          </View>
        </Dialog>
      </If>
    </>
  );
}
