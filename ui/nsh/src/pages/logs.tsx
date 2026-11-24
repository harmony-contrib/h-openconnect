import {
  batch,
  computed,
  effect,
  fontScale,
  List,
  platform,
  reduceMotion,
  Scroll,
  signal,
  Text,
  untracked,
  View,
  windowSize,
  type ListRef,
} from "nsh";
import {
  formatBytes,
  formatLogTime,
  formatTimestamp,
  sanitizeDisplayText,
} from "../locale";
import type { DiagnosticEntry, LogRecordingStatus } from "../model";
import { Action, Actions, IconButton } from "../ui/button";
import {
  confirmAction,
  copyText,
  RouteOwner,
  useApp,
  useLayout,
  useLifetime,
} from "../ui/context";
import { SearchField } from "../ui/field";
import { Icon } from "../ui/icons";
import { ObjectMenu } from "../ui/menu";
import { animateDisclosure, feedbackStyle, usePressFeedback } from "../ui/motion";
import { BottomSheet, SheetActionItem } from "../ui/overlay";
import {
  Badge,
  Code,
  Detail,
  Empty,
  IconTile,
  ListSurface,
  SectionHeader,
  Separator,
  Well,
} from "../ui/surface";
import { Tabs } from "../ui/tabs";
import {
  density,
  focusRing,
  palette,
  radius,
  spacing,
  surfaceFinish,
  typography,
} from "../ui/tokens";

function extractLogTarget(message: string): string | undefined {
  const urlMatch = message.match(/https?:\/\/[^\s\]\)"',;]+/i);
  if (urlMatch) return urlMatch[0];
  const hostMatch = message.match(
    /\b(?:[a-zA-Z0-9-]+\.)+[a-zA-Z]{2,}(?::\d{1,5})?\b/,
  );
  if (hostMatch) return hostMatch[0];
  const ipMatch = message.match(/\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{1,5})?\b/);
  return ipMatch ? ipMatch[0] : undefined;
}

function extractLogModule(
  message: string,
  t: (zh: string, en: string) => string,
): string {
  const bracketed = message.match(/^\[([a-zA-Z0-9_./ -]{2,32})\]/);
  if (bracketed?.[1]) return bracketed[1].trim();
  if (/\bdns\b/i.test(message)) return "DNS";
  if (/\b(?:tls|ssl|cert|dtls|cstp)\b/i.test(message))
    return t("加密传输", "Transport");
  if (/\b(?:auth|cookie|sso|saml|login)\b/i.test(message))
    return t("身份验证", "Authentication");
  if (/\b(?:tun|route)\b/i.test(message)) return t("网络隧道", "Tunnel");
  return t("连接服务", "Connection");
}

function matchesLog(entry: DiagnosticEntry, query: string, level: string): boolean {
  if (level !== "all") {
    const lv = entry.level.toLowerCase();
    if (level === "warn" && lv !== "warn" && lv !== "warning") return false;
    if (level === "error" && lv !== "error" && lv !== "fatal") return false;
    if (level !== "warn" && level !== "error" && lv !== level) return false;
  }
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    entry.message.toLowerCase().includes(q) ||
    entry.level.toLowerCase().includes(q)
  );
}

export function LogsPage() {
  const ui = useApp();
  const layout = useLayout();
  const owner = RouteOwner() || ui.route();
  const alive = useLifetime();

  const query = signal("");
  const level = signal("all");
  const history = signal(false);
  const selected = signal<DiagnosticEntry | undefined>(undefined);
  const pausedScroll = signal(false);
  const atEnd = signal(true);
  const seen = signal("");
  const pillPress = usePressFeedback(() => false, 0.96);
  let list: ListRef | undefined;

  const logsDomain = () => ui.client.domains.data("logs");
  const recording = (): LogRecordingStatus =>
    logsDomain()?.recording ?? { enabled: false, archives: [] };

  effect(() => {
    ui.logRecording.set(recording().enabled);
  });

  let lastIntent = ui.logRecordingIntent();
  effect(() => {
    const intent = ui.logRecordingIntent();
    if (ui.route() !== owner) return;
    if (intent !== lastIntent) {
      lastIntent = intent;
      untracked(() => {
        const nextEnabled = !recording().enabled;
        void ui.client
          .run<LogRecordingStatus>("logs.setRecording", {
            enabled: nextEnabled,
          })
          .then((data) => {
            if (alive() && data) {
              ui.logRecording.set(data.enabled);
              ui.client.pushNotice(
                data.enabled
                  ? ui.t("已开始记录日志", "Log recording started")
                  : ui.t("已停止记录日志", "Log recording stopped"),
              );
            }
          });
      });
    }
  });

  const rows = computed(() => {
    const duplicates = new Map<string, number>();
    return (logsDomain()?.entries || [])
      .map((row) => {
        const cleanMessage = sanitizeDisplayText(row.message);
        const identity = `${row.timestamp}:${row.level}:${cleanMessage}`;
        const count = duplicates.get(identity) || 0;
        duplicates.set(identity, count + 1);
        return { ...row, message: cleanMessage, key: `${identity}:${count}` };
      })
      .filter((row) => matchesLog(row, query(), level()));
  });

  const latest = () => rows().at(-1)?.key || "";
  effect(() => {
    const key = latest();
    if (atEnd() && !pausedScroll()) seen.set(key);
  });

  const unseenCount = () => {
    const marker = seen();
    const current = rows();
    if (!marker || current.length === 0) return 0;
    const index = current.findIndex((item) => item.key === marker);
    return index >= 0 ? Math.max(0, current.length - 1 - index) : 0;
  };

  const toEnd = () => {
    batch(() => {
      pausedScroll.set(false);
      atEnd.set(true);
      seen.set(latest());
    });
    list?.scrollToEnd({ animated: !reduceMotion() });
  };

  const levelTone = (value: string) => {
    const lv = value.toLowerCase();
    if (lv === "error" || lv === "fatal") return "danger" as const;
    if (lv === "warn" || lv === "warning") return "warning" as const;
    if (lv === "info") return "info" as const;
    return "neutral" as const;
  };

  const levelLabel = (value: string) => {
    const lv = value.toLowerCase();
    if (lv === "error" || lv === "fatal") return ui.t("错误", "ERROR");
    if (lv === "warn" || lv === "warning") return ui.t("警告", "WARN");
    if (lv === "info") return ui.t("信息", "INFO");
    if (lv === "debug") return ui.t("调试", "DEBUG");
    return value.toUpperCase();
  };

  const filtered = () => !!query() || level() !== "all";
  const clearFilters = () =>
    batch(() => {
      query.set("");
      level.set("all");
    });

  const splitPad = () => !platform.desktop && layout().masterDetail;
  const activeLog = () =>
    selected() || (splitPad() ? rows().at(-1) : undefined);
  const showDesktopInspector = () =>
    platform.desktop && layout().masterDetail && !!selected();
  const showSheet = () => !layout().masterDetail && !!selected();

  const selectLog = (entry: DiagnosticEntry) => {
    if (platform.desktop && layout().masterDetail && !selected()) {
      animateDisclosure();
    }
    selected.set(entry);
  };

  const closeSelected = () => {
    if (platform.desktop && layout().masterDetail && selected()) {
      animateDisclosure();
    }
    selected.set(undefined);
  };

  const exportArchive = async (fileName: string) => {
    await ui.client.exportLog(
      fileName,
      undefined,
      ui.t(`日志已导出：${fileName}`, `Log exported: ${fileName}`),
    );
  };

  const inlineToolbar = () => platform.desktop && layout().usable >= 720;

  const tabsControl = (inline?: boolean) => (
    <Tabs
      inline={inline}
      idPrefix="logs-tab"
      value={history() ? "history" : "current"}
      options={[
        { value: "current", label: ui.t("当前日志", "Current logs") },
        {
          value: "history",
          label: ui.t("历史归档", "Archives"),
          count: recording().archives?.length || 0,
        },
      ]}
      onChange={(tab) => {
        history.set(tab === "history");
        if (tab === "history") ui.client.domains.refresh(["logs"]);
      }}
    />
  );

  const levelTabs = (inline?: boolean) => (
    <Tabs
      size="small"
      inline={inline ?? false}
      idPrefix="logs-level"
      value={level()}
      options={[
        { value: "all", label: ui.t("全部", "All") },
        { value: "info", label: ui.t("信息", "Info") },
        { value: "warn", label: ui.t("警告", "Warn") },
        { value: "error", label: ui.t("错误", "Error") },
        { value: "debug", label: ui.t("调试", "Debug") },
      ]}
      onChange={(value) => level.set(value)}
    />
  );

  const statusNoteRow = () => (
    <View
      style={{
        minHeight: 22,
        flexDirection: "row",
        alignItems: "center",
        flexWrap: "wrap",
        gap: 6,
        paddingHorizontal: 2,
      }}
    >
      <View
        style={{
          width: 8,
          height: 8,
          borderRadius: radius.full,
          backgroundColor: recording().enabled
            ? palette().success
            : palette().faint,
        }}
      />
      <Text
        style={{
          fontSize: typography.note,
          lineHeight: density.line(18),
          fontWeight: "500",
          color: palette().muted,
        }}
      >
        {recording().enabled
          ? ui.t("正在记录并按天保存", "Recording daily")
          : ui.t("日志记录已关闭", "Log recording off")}
      </Text>
      <Text style={{ fontSize: typography.note, color: palette().faint }}>·</Text>
      <Text
        style={{
          fontSize: typography.note,
          lineHeight: density.line(18),
          color: palette().muted,
          fontVariant: ["tabular-nums"],
        }}
      >
        {`${recording().archives?.length || 0} ${ui.t("个日志文件", "log files")}`}
      </Text>
      <View style={{ flex: 1 }} />
      <If when={!history()}>
        <Text
          style={{
            fontSize: typography.note,
            lineHeight: density.line(18),
            color: palette().muted,
            fontVariant: ["tabular-nums"],
          }}
        >
          {`${rows().length} ${ui.t("条", "entries")}`}
        </Text>
      </If>
      <If when={!history() && filtered()}>
        <Action
          id="logs-clear-filters"
          variant="link"
          small
          label={ui.t("清除筛选", "Clear filters")}
          onPress={clearFilters}
        />
      </If>
    </View>
  );

  const currentLogStream = () => (
    <View style={{ flex: 1, minHeight: 0, position: "relative" }}>
      <List
        testID="logs-list"
        ref={(node) => {
          list = node;
        }}
        maintainVisibleContentPosition={
          pausedScroll() ? {} : { autoscrollToBottomThreshold: 24 }
        }
        onScroll={(event) => {
          const bottom =
            event.contentSize.height - event.offset.y - event.size.height <= 24;
          batch(() => {
            atEnd.set(bottom);
            if (bottom) {
              pausedScroll.set(false);
              seen.set(latest());
            } else {
              pausedScroll.set(true);
            }
          });
        }}
        bounces={false}
        keyboardDismissMode="on-drag"
        keyboardShouldPersistTaps="handled"
        each={rows()}
        keyExtractor={(row) => row.key}
        estimatedItemSize={48}
        style={{
          flex: 1,
          borderWidth: 1,
          borderColor: palette().border,
          borderRadius: radius.xl,
          borderCurve: "continuous",
          overflow: "hidden",
          backgroundColor: palette().panel,
          ...surfaceFinish("card"),
        }}
        contentStyle={{
          paddingTop: 2,
          paddingBottom: !atEnd() || pausedScroll() ? 58 : 4,
        }}
        empty={
          <Empty
            icon="scroll-text"
            title={
              filtered()
                ? ui.t("没有匹配日志", "No matching logs")
                : ui.t("暂无日志", "No logs")
            }
            text={
              filtered()
                ? ui.t(
                    "调整筛选条件后继续查看。",
                    "Adjust filters to continue.",
                  )
                : recording().enabled
                  ? ui.t(
                      "连接事件将显示在这里。",
                      "Connection events appear here.",
                    )
                  : ui.t(
                      "开启日志记录后，连接事件将显示在这里。",
                      "Start recording to view connection events here.",
                    )
            }
          >
            <If when={filtered()}>
              <Action
                small
                secondary
                label={ui.t("清除筛选", "Clear filters")}
                onPress={clearFilters}
              />
            </If>
          </Empty>
        }
      >
        {(row) => {
          const isSelected = () =>
            activeLog()?.timestamp === row().timestamp &&
            activeLog()?.message === row().message &&
            (!!selected() || splitPad());
          return (
            <View
              testID={`log-row-${row().timestamp}`}
              role="button"
              accessibilityLabel={`${levelLabel(row().level)} ${formatTimestamp(row().timestamp)}`}
              focusVisibleStyle={focusRing(0)}
              hoverStyle={{
                backgroundColor: isSelected()
                  ? palette().accentFill
                  : palette().hover,
              }}
              activeStyle={{
                backgroundColor: isSelected()
                  ? palette().accentFill
                  : palette().soft,
              }}
              onPress={() => selectLog(row())}
              style={{
                minHeight: Math.max(
                  46,
                  density.line(20) * Math.max(1, fontScale()) + 20,
                ),
                paddingHorizontal: 14,
                paddingVertical: 10,
                flexDirection: "row",
                alignItems: "flex-start",
                gap: 10,
                borderBottomWidth: 1,
                borderBottomColor: palette().line,
                backgroundColor: isSelected()
                  ? palette().accentFill
                  : "transparent",
                ...feedbackStyle(),
              }}
            >
              <Text
                style={{
                  flexShrink: 0,
                  fontFamily: "monospace",
                  fontSize: density.font(12.5),
                  lineHeight: density.line(20),
                  fontVariant: ["tabular-nums"],
                  color: palette().faint,
                }}
              >
                {formatLogTime(row().timestamp)}
              </Text>
              <Badge
                tone={levelTone(row().level)}
                label={levelLabel(row().level)}
              />
              <Text
                style={{
                  flex: 1,
                  minWidth: 0,
                  numberOfLines: 2,
                  fontFamily: "monospace",
                  fontSize: density.font(13.5),
                  lineHeight: density.line(20),
                  color: palette().code,
                }}
              >
                {row().message.replace(/[\n\r]+/g, " ").slice(0, 200)}
              </Text>
            </View>
          );
        }}
      </List>

      <If when={!atEnd() || pausedScroll()}>
        <View
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            bottom: 14,
            alignItems: "center",
            pointerEvents: "box-none",
          }}
        >
          <View
            testID="logs-to-bottom"
            role="button"
            onPressDown={pillPress.down}
            onPressUp={pillPress.up}
            focusVisibleStyle={focusRing()}
            hoverStyle={surfaceFinish("hover")}
            activeStyle={surfaceFinish("pressed")}
            onPress={toEnd}
            style={{
              minHeight: 36,
              paddingHorizontal: 14,
              borderRadius: radius.full,
              borderCurve: "continuous",
              borderWidth: 1,
              borderColor: palette().border,
              backgroundColor: palette().raised,
              flexDirection: "row",
              alignItems: "center",
              gap: 6,
              ...surfaceFinish("menu"),
              ...feedbackStyle(),
              transform: [{ scale: pillPress.scale }],
            }}
          >
            <Icon name="arrow-down" size={14} color={palette().accent} />
            <Text
              style={{
                numberOfLines: 1,
                flexShrink: 0,
                fontSize: typography.note,
                lineHeight: density.line(18),
                fontWeight: "600",
                color: palette().text,
                fontVariant: ["tabular-nums"],
              }}
            >
              {unseenCount() > 0
                ? ui.t(`${unseenCount()} 条新日志`, `${unseenCount()} new logs`)
                : ui.t("回到底部", "Jump to bottom")}
            </Text>
          </View>
        </View>
      </If>
    </View>
  );

  const archiveList = () => (
    <List
      testID="logs-archives-list"
      bounces={false}
      keyboardDismissMode="on-drag"
      keyboardShouldPersistTaps="handled"
      each={recording().archives || []}
      keyExtractor={(row) => row.fileName}
      estimatedItemSize={60}
      style={{
        flex: 1,
        borderWidth: 1,
        borderColor: palette().border,
        borderRadius: radius.xl,
        borderCurve: "continuous",
        overflow: "hidden",
        backgroundColor: palette().panel,
        ...surfaceFinish("card"),
      }}
      contentStyle={{ paddingBottom: spacing.xxs }}
      empty={
        <Empty
          icon="history"
          title={ui.t("暂无历史归档", "No log archives")}
          text={ui.t(
            "开启记录后按日保存的日志会显示在这里。",
            "Daily log files appear here after recording starts.",
          )}
        />
      }
    >
      {(archive) => (
        <ObjectMenu
          id={`archive-menu-${archive().fileName}`}
          title={archive().fileName}
          subtitle={
            archive().active
              ? ui.t("当前正在写入", "Currently recording")
              : undefined
          }
          commands={[
            {
              id: `archive-export-${archive().fileName}`,
              label: ui.t("导出日志文件…", "Export log…"),
              icon: "download",
              disabled: ui.client.pending() > 0,
              run: () => void exportArchive(archive().fileName),
            },
            {
              id: `archive-delete-${archive().fileName}`,
              label: ui.t("删除归档…", "Delete archive…"),
              icon: "trash-2",
              danger: true,
              disabled: archive().active || ui.client.pending() > 0,
              disabledReason: archive().active
                ? ui.t("正在写入，不可删除", "Active file")
                : undefined,
              run: () => {
                const fileName = archive().fileName;
                void confirmAction(
                  ui,
                  ui.t(`删除“${fileName}”？`, `Delete “${fileName}”?`),
                  async () => {
                    await ui.client.run(
                      "logs.deleteArchive",
                      { fileName },
                      ui.t(`日志已删除：${fileName}`, `Log deleted: ${fileName}`),
                    );
                  },
                  { actionLabel: ui.t("删除日志", "Delete log") },
                );
              },
            },
          ]}
          row={(menu) => (
            <View
              style={{
                minHeight: Math.max(
                  56,
                  density.line(38) * Math.max(1, fontScale()) + 20,
                ),
                paddingHorizontal: 14,
                paddingVertical: 10,
                gap: spacing.md,
                flexDirection: "row",
                alignItems: "center",
                borderBottomWidth: 1,
                borderBottomColor: palette().line,
                backgroundColor: palette().panel,
              }}
            >
              <IconTile icon="file-text" />
              <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
                <View
                  style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 8,
                    minWidth: 0,
                  }}
                >
                  <Text
                    style={{
                      flexShrink: 1,
                      minWidth: 0,
                      numberOfLines: 1,
                      fontSize: typography.sm,
                      lineHeight: density.line(20),
                      fontWeight: "600",
                      color: palette().text,
                    }}
                  >
                    {archive().fileName}
                  </Text>
                  <If when={archive().active}>
                    <Badge
                      tone="brand"
                      label={ui.t("正在写入", "Recording")}
                    />
                  </If>
                </View>
                <Text
                  style={{
                    numberOfLines: 1,
                    fontSize: typography.note,
                    lineHeight: density.line(18),
                    color: palette().muted,
                    fontVariant: ["tabular-nums"],
                  }}
                >
                  {formatBytes(archive().bytes)} ·{" "}
                  {formatTimestamp(archive().updatedAt) === "—"
                    ? archive().date || "—"
                    : formatTimestamp(archive().updatedAt)}
                </Text>
              </View>
              {menu}
            </View>
          )}
        />
      )}
    </List>
  );

  const LogSheet = () => {
    const entry = () => selected()!;
    const target = () => extractLogTarget(entry().message);
    return (
      <BottomSheet
        title={ui.t("日志详情", "Log details")}
        subtitle={`${formatTimestamp(entry().timestamp)} · ${levelLabel(entry().level)}`}
        onClose={closeSelected}
      >
        <Well style={{ padding: 0, overflow: "hidden" }}>
          <Scroll
            bounces={false}
            style={{
              maxHeight: Math.min(240, Math.max(120, windowSize().height * 0.28)),
            }}
            contentStyle={{ padding: 12 }}
          >
            <Code text={entry().message} />
          </Scroll>
        </Well>
        <ListSurface style={{ padding: spacing.md, gap: spacing.sm }}>
          <Detail
            label={ui.t("级别", "Level")}
            value={`${levelLabel(entry().level)} (${entry().level.toUpperCase()})`}
            tone={levelTone(entry().level)}
          />
          <Separator />
          <Detail
            label={ui.t("模块", "Module")}
            value={extractLogModule(entry().message, ui.t)}
          />
          <Separator />
          <Detail
            stacked={(target()?.length || 0) > 26}
            label={ui.t("关联目标", "Target")}
            value={target() || "—"}
          />
        </ListSurface>
        <ListSurface style={{ padding: 6, gap: 2 }}>
          <SheetActionItem
            id="log-copy-entry"
            icon="copy"
            label={ui.t("复制此条日志", "Copy log entry")}
            onPress={() => {
              copyText(
                ui,
                entry().message,
                ui.t("已复制日志", "Log copied"),
              );
              closeSelected();
            }}
          />
        </ListSurface>
      </BottomSheet>
    );
  };

  return (
    <View
      style={{
        flex: 1,
        paddingHorizontal: layout().inset,
        paddingTop: layout().inset,
        paddingBottom: spacing.md,
        gap: 12,
      }}
    >
      <If
        when={!history() && splitPad()}
        else={
          <>
            <If
              when={inlineToolbar()}
              else={
                <View style={{ gap: 10 }}>
                  {tabsControl(false)}
                  {statusNoteRow()}
                  <If when={!history()}>
                    <SearchField
                      id="logs-search"
                      label={ui.t("搜索日志", "Search logs")}
                      placeholder={ui.t(
                        "搜索关键词、地址或错误",
                        "Search keyword, host or error",
                      )}
                      value={query()}
                      onChange={(value) => query.set(value)}
                    />
                    {levelTabs(false)}
                  </If>
                </View>
              }
            >
              <View style={{ gap: 8 }}>
                <View
                  style={{
                    flexDirection: "row",
                    flexWrap: "wrap",
                    alignItems: "center",
                    gap: 10,
                  }}
                >
                  {tabsControl(true)}
                  <If when={!history()}>
                    <View style={{ flex: 1, minWidth: 200 }}>
                      <SearchField
                        id="logs-search"
                        label={ui.t("搜索日志", "Search logs")}
                        placeholder={ui.t(
                          "搜索关键词、地址或错误",
                          "Search keyword, host or error",
                        )}
                        value={query()}
                        onChange={(value) => query.set(value)}
                      />
                    </View>
                  </If>
                </View>
                <If when={!history()} else={statusNoteRow()}>
                  <View
                    style={{
                      flexDirection: "row",
                      flexWrap: "wrap",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 10,
                    }}
                  >
                    {levelTabs(true)}
                    <View style={{ flex: 1, minWidth: 200 }}>
                      {statusNoteRow()}
                    </View>
                  </View>
                </If>
              </View>
            </If>

            <If
              when={history()}
              else={
                <View
                  style={{
                    flex: 1,
                    minHeight: 0,
                    flexDirection: "row",
                    gap: layout().gap,
                  }}
                >
                  <View style={{ flex: 1, minWidth: 0 }}>
                    {currentLogStream()}
                  </View>
                  <If when={showDesktopInspector()}>
                    <View
                      testID="logs-inspector"
                      style={{
                        width: 330,
                        flexShrink: 0,
                        borderWidth: 1,
                        borderColor: palette().border,
                        borderRadius: radius.xl,
                        borderCurve: "continuous",
                        backgroundColor: palette().panel,
                        overflow: "hidden",
                        ...surfaceFinish("card"),
                      }}
                    >
                      <Scroll
                        bounces={false}
                        style={{ flex: 1 }}
                        contentStyle={{ padding: spacing.lg, gap: spacing.md }}
                      >
                        <LogInspector
                          entry={selected()!}
                          title={ui.t("日志详情", "Log details")}
                          levelTone={levelTone(selected()!.level)}
                          levelLabel={levelLabel(selected()!.level)}
                          onClose={closeSelected}
                        />
                      </Scroll>
                    </View>
                  </If>
                </View>
              }
            >
              {archiveList()}
            </If>
          </>
        }
      >
        <View
          style={{
            flex: 1,
            minHeight: 0,
            flexDirection: "row",
            gap: layout().gap,
          }}
        >
          <View style={{ flex: 1.15, minWidth: 0, gap: 10 }}>
            {tabsControl(false)}
            {statusNoteRow()}
            <SearchField
              id="logs-search"
              label={ui.t("搜索日志", "Search logs")}
              placeholder={ui.t(
                "搜索关键词、地址或错误",
                "Search keyword, host or error",
              )}
              value={query()}
              onChange={(value) => query.set(value)}
            />
            {levelTabs(false)}
            {currentLogStream()}
          </View>
          <View style={{ flex: 0.85, minWidth: 300 }}>
            <If
              when={!!activeLog()}
              else={
                <ListSurface style={{ flex: 1, justifyContent: "center" }}>
                  <Empty
                    icon="scroll-text"
                    title={ui.t("未选择日志", "No log selected")}
                    text={ui.t(
                      "在左侧点按任一条日志以查看详情。",
                      "Select a log entry on the left to inspect.",
                    )}
                  />
                </ListSurface>
              }
            >
              <Scroll
                bounces={false}
                style={{ flex: 1 }}
                contentStyle={{ gap: spacing.md, paddingBottom: spacing.lg }}
              >
                <LogInspector
                  entry={activeLog()!}
                  title={ui.t("日志详情", "Log details")}
                  levelTone={levelTone(activeLog()!.level)}
                  levelLabel={levelLabel(activeLog()!.level)}
                  onClose={selected() ? closeSelected : undefined}
                />
              </Scroll>
            </If>
          </View>
        </View>
      </If>

      <If when={showSheet()}>
        <LogSheet />
      </If>
    </View>
  );
}

function LogInspector(props: {
  entry: DiagnosticEntry;
  title: string;
  levelTone: "danger" | "warning" | "info" | "neutral";
  levelLabel: string;
  onClose?: () => void;
}) {
  const ui = useApp();
  const target = () => extractLogTarget(props.entry.message);
  return (
    <View style={{ gap: spacing.md }}>
      <SectionHeader
        title={props.title}
        right={
          <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
            <Badge tone={props.levelTone} label={props.levelLabel} />
            <If when={!!props.onClose}>
              <IconButton
                id="dialog-close"
                icon="x"
                small
                iconColor={palette().muted}
                label={ui.t("关闭详情", "Close details")}
                onPress={() => props.onClose?.()}
              />
            </If>
          </View>
        }
      />
      <Well style={{ padding: 0, overflow: "hidden" }}>
        <Scroll
          bounces={false}
          style={{
            maxHeight: Math.min(240, Math.max(120, windowSize().height * 0.28)),
          }}
          contentStyle={{ padding: 12 }}
        >
          <Code text={props.entry.message} />
        </Scroll>
      </Well>
      <ListSurface style={{ padding: spacing.md, gap: spacing.sm }}>
        <Detail
          label={ui.t("时间", "Time")}
          value={formatTimestamp(props.entry.timestamp)}
        />
        <Separator />
        <Detail
          label={ui.t("级别", "Level")}
          value={`${props.levelLabel} (${props.entry.level.toUpperCase()})`}
          tone={props.levelTone}
        />
        <Separator />
        <Detail
          label={ui.t("模块", "Module")}
          value={extractLogModule(props.entry.message, ui.t)}
        />
        <Separator />
        <Detail
          stacked={!!target() && target()!.length > 24}
          label={ui.t("关联目标", "Target")}
          value={target() || "—"}
        />
      </ListSurface>
      <Actions align="start">
        <Action
          id="log-inspector-copy"
          small
          secondary
          icon="copy"
          label={ui.t("复制此条日志", "Copy log entry")}
          onPress={() =>
            copyText(
              ui,
              props.entry.message,
              ui.t("已复制日志", "Log copied"),
            )
          }
        />
      </Actions>
    </View>
  );
}
