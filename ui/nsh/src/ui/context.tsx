import {
  Clipboard,
  cleanup,
  context,
  effect,
  signal,
  untracked,
  type Signal,
} from "nsh";
import { AlertDialog } from "nsh/base";
import type { CoreClient } from "../client";
import type { WorkspaceLayout } from "../layout";
import { resolveLanguage } from "../locale";

export type Page =
  | "home"
  | "profiles"
  | "profile-editor"
  | "statistics"
  | "logs"
  | "settings"
  | "theme"
  | "language"
  | "about"
  | "privacy";

export interface AppContext {
  client: CoreClient;
  route: () => unknown;
  page: () => Page;
  navigate: (page: Page, query?: string) => void;
  switchWorkspace: (page: Page) => void;
  back: () => void;
  t: (zh: string, en: string) => string;
  isZh: () => boolean;
  setOverlayDelta: (delta: number) => void;
  registerLeaveGuard: (
    owner: unknown,
    guard: () => boolean | Promise<boolean>,
  ) => () => void;
  logRecording: Signal<boolean>;
  logRecordingIntent: Signal<number>;
  openProfileEditor: (profileId?: string) => void;
}

export const App = context<AppContext | undefined>(undefined);
export const RouteLayout = context<(() => WorkspaceLayout) | undefined>(undefined);
export const RouteOwner = context<unknown>(undefined);

export function useApp(): AppContext {
  const value = App();
  if (!value) throw new Error("App context is missing");
  return value;
}

export function useLayout(): () => WorkspaceLayout {
  const value = RouteLayout();
  if (!value) throw new Error("RouteLayout context is missing");
  return value;
}

export function useLifetime(): () => boolean {
  const alive = signal(true);
  cleanup(() => alive.set(false));
  return () => alive();
}

export function useOverlay(open: () => boolean): void {
  const ui = App();
  if (!ui) return;
  let active = false;
  effect(() => {
    const next = open();
    if (next !== active) {
      active = next;
      untracked(() => ui.setOverlayDelta(next ? 1 : -1));
    }
  });
  cleanup(() => {
    if (active) {
      active = false;
      untracked(() => ui.setOverlayDelta(-1));
    }
  });
}

export async function askConfirmation(
  ui: AppContext,
  title: string,
  options?: {
    description?: string;
    actionLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
  },
): Promise<boolean> {
  const route = ui.route();
  ui.setOverlayDelta(1);
  try {
    const result = await AlertDialog.show({
      title,
      description: options?.description,
      closeOnEscape: true,
      actions: [
        {
          id: "cancel",
          label: options?.cancelLabel || ui.t("取消", "Cancel"),
          role: "cancel",
        },
        {
          id: "commit",
          label: options?.actionLabel || ui.t("确定", "Confirm"),
          role: options?.destructive === false ? "default" : "destructive",
        },
      ],
    });
    return result.action === "commit" && ui.route() === route;
  } finally {
    ui.setOverlayDelta(-1);
  }
}

export function useDraftGuard(
  isDirty: () => boolean,
  onDiscard: () => void,
  isSaving?: () => boolean,
): void {
  const ui = App();
  const owner = RouteOwner();
  if (!ui || !owner) return;
  const unregister = ui.registerLeaveGuard(owner, async () => {
    if (isSaving?.()) return false;
    if (!isDirty()) return true;
    const accepted = await askConfirmation(
      ui,
      ui.t("放弃未保存的更改？", "Discard unsaved changes?"),
      {
        description: ui.t(
          "离开此页面将丢失当前编辑内容。",
          "Leaving now will discard your current edits.",
        ),
        actionLabel: ui.t("放弃修改", "Discard"),
        cancelLabel: ui.t("继续编辑", "Keep editing"),
        destructive: true,
      },
    );
    if (accepted) {
      onDiscard();
      return true;
    }
    return false;
  });
  cleanup(unregister);
}

export async function confirmAction(
  ui: AppContext,
  title: string,
  onConfirm: () => void | Promise<void>,
  options?: {
    description?: string;
    actionLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
  },
): Promise<void> {
  const accepted = await askConfirmation(ui, title, options);
  if (accepted) {
    await onConfirm();
  }
}

export function copyText(ui: AppContext, text: string, notice: string): void {
  Clipboard.writeText(text);
  ui.client.pushNotice(notice);
}

export function makeTranslator(client: CoreClient) {
  const isZh = () => {
    const prefs = client.preferences();
    return resolveLanguage(prefs.language, prefs.systemLocale) === "zh";
  };
  const t = (zh: string, en: string) => (isZh() ? zh : en);
  return { t, isZh };
}
