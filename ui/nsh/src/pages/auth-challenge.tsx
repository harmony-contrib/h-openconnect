import { effect, signal, Text, View } from "nsh";
import { sanitizeDisplayText } from "../locale";
import {
  fieldKeyString,
  type AuthChallenge,
  type AuthField,
  type AuthFieldValue,
} from "../model";
import { Action, Actions } from "../ui/button";
import { useApp } from "../ui/context";
import { Field } from "../ui/field";
import { Icon } from "../ui/icons";
import { Select } from "../ui/menu";
import { BottomSheet } from "../ui/overlay";
import { Alert } from "../ui/surface";
import { density, palette, radius, spacing, typography } from "../ui/tokens";

export function AuthChallengeOverlay() {
  const ui = useApp();
  const challenge = (): AuthChallenge | null =>
    ui.client.domains.data("status")?.pendingAuth ?? null;

  const seededId = signal<number | null>(null);
  const values = signal<Record<string, string>>({});

  effect(() => {
    const active = challenge();
    if (!active) {
      seededId.set(null);
      values.set({});
      return;
    }
    if (seededId() !== active.id) {
      const next: Record<string, string> = {};
      for (const field of active.fields) {
        if (field.kind === "hidden") continue;
        next[fieldKeyString(field.key)] = field.value || "";
      }
      values.set(next);
      seededId.set(active.id);
    }
  });

  const setFieldValue = (field: AuthField, val: string) => {
    const key = fieldKeyString(field.key);
    values.update((map) => ({ ...map, [key]: val }));
  };

  const submit = async () => {
    const active = challenge();
    if (!active) return;
    const currentValues = values();
    const payloadValues: AuthFieldValue[] = [];
    for (const field of active.fields) {
      if (field.kind === "hidden") continue;
      const val = currentValues[fieldKeyString(field.key)] ?? "";
      if (field.required && !val.trim()) {
        ui.client.pushError(
          ui.t("请填写所有必填验证项", "Please complete all required fields"),
        );
        return;
      }
      if (val.trim()) {
        payloadValues.push({ key: field.key, value: val });
      }
    }
    if (payloadValues.length === 0) {
      ui.client.pushError(
        ui.t("请输入验证信息", "Please enter your verification response"),
      );
      return;
    }
    await ui.client.run("auth.submit", {
      id: active.id,
      values: payloadValues,
    });
  };

  const cancel = () => {
    void ui.client.run("auth.cancel");
  };

  return (
    <If when={!!challenge()}>
      {(() => {
        const active = () => challenge()!;
        const visibleFields = () =>
          active().fields.filter((f) => f.kind !== "hidden");
        const activeServer = () => {
          const profilesData = ui.client.domains.data("profiles");
          const activeId =
            profilesData?.activeConnectionId ??
            ui.client.domains.data("config")?.activeConnectionId ??
            "";
          return (
            profilesData?.profiles.find((p) => p.id === activeId)?.server || ""
          );
        };
        const subtitle = () => {
          const server = activeServer();
          const step =
            active().round > 1
              ? ui.t(`第 ${active().round} 步验证`, `Step ${active().round}`)
              : "";
          if (server && step) return `${server} · ${step}`;
          return server || step || undefined;
        };
        const promptLines = () =>
          [active().banner, active().message]
            .map((line) => sanitizeDisplayText(line || ""))
            .filter(Boolean);
        return (
          <BottomSheet
            title={ui.t("身份验证", "Verification")}
            subtitle={subtitle()}
            onClose={cancel}
            footer={
              <Actions>
                <Action
                  id="auth-cancel"
                  secondary
                  label={ui.t("取消", "Cancel")}
                  onPress={cancel}
                />
                <Action
                  id="auth-submit"
                  primary
                  icon="check"
                  loading={ui.client.isPending("auth.submit")}
                  label={ui.t("提交验证", "Verify")}
                  onPress={() => void submit()}
                />
              </Actions>
            }
          >
            <View testID="auth-challenge-form" style={{ gap: spacing.md }}>
              <If when={!!active().error}>
                <Alert
                  danger
                  text={sanitizeDisplayText(active().error || "")}
                />
              </If>
              <If when={promptLines().length > 0}>
                <View
                  testID="auth-prompt-banner"
                  style={{
                    flexDirection: "row",
                    alignItems: "flex-start",
                    gap: spacing.sm,
                    paddingHorizontal: spacing.md,
                    paddingVertical: density.space(10),
                    borderRadius: radius.lg,
                    borderCurve: "continuous",
                    backgroundColor: palette().accentFill,
                  }}
                >
                  <View style={{ marginTop: 1 }}>
                    <Icon
                      name="shield-check"
                      size={16}
                      color={palette().accent}
                    />
                  </View>
                  <View style={{ flex: 1, minWidth: 0, gap: spacing.xxs }}>
                    <Each of={promptLines()}>
                      {(line) => (
                        <Text
                          style={{
                            fontSize: typography.note,
                            lineHeight: density.line(18),
                            color: palette().text,
                          }}
                        >
                          {line()}
                        </Text>
                      )}
                    </Each>
                  </View>
                </View>
              </If>

              <Each of={visibleFields()} key={(f) => fieldKeyString(f.key)}>
                {(field) => {
                  const key = () => fieldKeyString(field().key);
                  const val = () => values()[key()] ?? "";
                  const label = () =>
                    sanitizeDisplayText(field().label || field().name);
                  return (
                    <If
                      when={field().kind === "select" && field().choices.length > 0}
                      else={
                        <Field
                          id={`auth-field-${field().key.optionIndex}`}
                          label={label()}
                          value={val()}
                          password={
                            field().kind === "password" ||
                            field().kind === "token"
                          }
                          onChange={(next) => setFieldValue(field(), next)}
                          onSubmit={() => void submit()}
                        />
                      }
                    >
                      <Select
                        id={`auth-select-${field().key.optionIndex}`}
                        label={label()}
                        value={val()}
                        options={field().choices.map((c) => ({
                          value: c.name,
                          label: sanitizeDisplayText(c.label || c.name),
                        }))}
                        onChange={(next) => setFieldValue(field(), next)}
                      />
                    </If>
                  );
                }}
              </Each>
            </View>
          </BottomSheet>
        );
      })()}
    </If>
  );
}
