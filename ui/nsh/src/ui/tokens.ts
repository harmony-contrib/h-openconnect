import {
  token,
  type Tokens,
  type ViewStyle,
  colorScheme,
  platform,
  fontScale,
  reduceMotion,
  safeArea,
  windowSize,
} from "nsh";
import finishes from "../finishes.json";

export const density = {
  font(size: number) {
    return size;
  },
  line(size: number) {
    return size;
  },
  space(size: number) {
    return size;
  },
  icon(size: number) {
    return size;
  },
};

export const spacing = {
  get xxs() {
    return density.space(4);
  },
  get xs() {
    return density.space(4);
  },
  get sm() {
    return density.space(8);
  },
  get md() {
    return density.space(12);
  },
  get lg() {
    return density.space(16);
  },
  get xl() {
    return density.space(24);
  },
  get xxl() {
    return density.space(24);
  },
  get section() {
    return density.space(20);
  },
  get region() {
    return density.space(32);
  },
};

const pointer = () => platform.desktop;

export const radius = {
  get sm() {
    return pointer() ? 5 : token("radius-small");
  },
  get md() {
    return pointer() ? 7 : token("radius-control");
  },
  get lg() {
    return pointer() ? 10 : token("radius");
  },
  get xl() {
    return pointer() ? 10 : token("radius-card");
  },
  get xxl() {
    return token("radius-dialog");
  },
  sheet: 20,
  full: 999,
};

export const typography = {
  get xs() {
    return density.font(12);
  },
  get note() {
    return density.font(pointer() ? 12 : 13);
  },
  get sm() {
    return density.font(pointer() ? 14 : 15);
  },
  get section() {
    return density.font(pointer() ? 13 : 15);
  },
  get md() {
    return density.font(17);
  },
  get lg() {
    return density.font(18);
  },
  get xl() {
    return density.font(20);
  },
  get xxl() {
    return density.font(24);
  },
};

export const control = {
  get height() {
    return pointer() ? 28 : 40;
  },
  get small() {
    return pointer() ? 24 : 36;
  },
  get large() {
    return pointer() ? 34 : 46;
  },
  get icon() {
    return pointer() ? 28 : 40;
  },
  get field() {
    return pointer() ? 32 : 46;
  },
  get select() {
    return pointer() ? 32 : 44;
  },
  get segment() {
    return pointer() ? 30 : 36;
  },
  get segmentSmall() {
    return pointer() ? 24 : 32;
  },
  get box() {
    return pointer() ? 16 : 22;
  },
  get menuRow() {
    return pointer() ? 26 : 46;
  },
  get menuRadius() {
    return pointer() ? 6 : 10;
  },
  get rowPadding() {
    return pointer() ? 6 : 12;
  },
  get row() {
    return Math.max(
      pointer() ? 40 : 56,
      density.line(20) * Math.max(1, fontScale()) + (pointer() ? 16 : 24),
    );
  },
  hitSlop(size: number) {
    return !pointer() ? Math.max(0, (44 - size) / 2) : 0;
  },
};

export const chrome = {
  header: 52,
  toolbar: 48,
  bottom: 56,
};

const names = {
  background: "background",
  panel: "card",
  text: "foreground",
  muted: "muted-foreground",
  faint: "faint",
  border: "border",
  accent: "brand",
  primaryText: "on-brand",
  brandFill: "brand-fill",
  stopFill: "stop-fill",
  onStop: "on-stop",
  accentFill: "brand-tint",
  soft: "muted",
  hover: "hover",
  field: "field",
  fieldBorder: "input",
  raised: "raised",
  track: "track",
  knob: "knob",
  code: "code",
  info: "info",
  infoFill: "info-tint",
  danger: "destructive",
  dangerText: "destructive-foreground",
  dangerFill: "destructive-tint",
  success: "up",
  successFill: "up-tint",
  warning: "warn",
  warningFill: "warn-tint",
  ring: "ring",
  sidebar: "sidebar",
  sidebarAccent: "sidebar-accent",
  sidebarBorder: "sidebar-border",
  inset: "inset",
  line: "line",
  popover: "popover",
  scrim: "overlay-scrim",
  chartDownload: "chart-1",
  chartUpload: "chart-2",
} satisfies Record<string, keyof Tokens>;

const colors = {} as Record<keyof typeof names, string>;
for (const name of Object.keys(names) as (keyof typeof names)[]) {
  Object.defineProperty(colors, name, {
    enumerable: true,
    get: () => token(names[name]),
  });
}

export function palette() {
  return colors;
}

export function surfaceFinish(kind: keyof typeof finishes.light): ViewStyle {
  return (colorScheme() === "dark" ? finishes.dark : finishes.light)[kind];
}

export function focusRing(offset: number | boolean = 1): ViewStyle {
  const outlineOffset =
    typeof offset === "boolean" ? (offset ? 0 : 2) : offset;
  return {
    outlineStyle: "solid",
    outlineWidth: 2,
    outlineColor: colors.ring,
    outlineOffset,
  };
}

export function useWidePopover(): boolean {
  return !platform.desktop && windowSize().width >= 768;
}

export function popupSurface(
  presentation?: "anchored" | "sheet",
  maxWidth?: number,
): ViewStyle {
  const menuAsSheet =
    presentation === "sheet" ||
    (presentation !== "anchored" &&
      !platform.desktop &&
      windowSize().width < 768);
  return {
    minWidth: 220,
    maxWidth: menuAsSheet ? undefined : (maxWidth ?? 340),
    padding: spacing.xxs,
    borderRadius: radius.lg,
    borderCurve: "continuous",
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.popover,
    ...(menuAsSheet
      ? {
          borderTopLeftRadius: radius.sheet,
          borderTopRightRadius: radius.sheet,
          borderBottomLeftRadius: 0,
          borderBottomRightRadius: 0,
          borderBottomWidth: 0,
          paddingBottom: Math.max(spacing.sm, safeArea().bottom),
        }
      : {}),
    ...surfaceFinish("menu"),
    transitionProperty: "opacity, transform",
    transitionDuration: reduceMotion() ? 0 : 150,
    transitionTimingFunction: "cubic-bezier(0.16, 1, 0.3, 1)",
  };
}

export type StatusTone =
  | "neutral"
  | "info"
  | "brand"
  | "success"
  | "warning"
  | "danger";

export function statusColors(tone: StatusTone = "neutral") {
  const text =
    tone === "brand"
      ? colors.accent
      : tone === "info"
        ? colors.info
        : tone === "success"
          ? colors.success
          : tone === "warning"
            ? colors.warning
            : tone === "danger"
              ? colors.danger
              : colors.muted;
  const background = token(
    tone === "brand"
      ? "brand-tint"
      : tone === "info"
        ? "info-tint"
        : tone === "success"
          ? "up-tint"
          : tone === "warning"
            ? "warn-tint"
            : tone === "danger"
              ? "destructive-tint"
              : "muted",
  );
  return { text, background, border: colors.border };
}
