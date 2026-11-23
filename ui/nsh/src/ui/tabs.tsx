import { platform, Text } from "nsh";
import { Tab, Tabs as NativeTabs } from "nsh/base";
import { feedbackStyle } from "./motion";
import {
  control,
  density,
  focusRing,
  palette,
  surfaceFinish,
  typography,
} from "./tokens";

export function Tabs(props: {
  options: { value: string; label: string; count?: number }[];
  value: string;
  onChange: (value: string) => void;
  idPrefix?: string;
  disabled?: boolean;
  inline?: boolean;
  flex?: number;
  size?: "regular" | "small";
}) {
  const inline = () =>
    props.flex !== undefined
      ? false
      : (props.inline ?? props.size === "small");
  const small = () => props.size === "small";
  return (
    <NativeTabs
      style={{
        flex: props.flex,
        minWidth: props.flex !== undefined ? 0 : undefined,
        alignSelf:
          props.flex !== undefined
            ? "auto"
            : inline()
              ? "flex-start"
              : "stretch",
        maxWidth: "100%",
        minHeight: small() ? control.segmentSmall : control.segment,
        padding: 2,
        flexDirection: "row",
        backgroundColor: palette().inset,
        borderRadius: platform.desktop ? 9 : 10,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: palette().border,
        ...surfaceFinish("track"),
      }}
      indicatorStyle={{
        top: 2,
        bottom: 2,
        borderRadius: platform.desktop ? 6 : 7,
        borderCurve: "continuous",
        borderWidth: 1,
        ...surfaceFinish("thumb"),
      }}
    >
      <Each of={props.options} key={(option) => option.value}>
        {(option) => {
          const active = () => props.value === option().value;
          return (
            <Tab
              focusVisibleStyle={focusRing(0)}
              hoverStyle={
                props.disabled || active()
                  ? undefined
                  : { backgroundColor: palette().hover }
              }
              activeStyle={
                props.disabled ? undefined : { backgroundColor: palette().soft }
              }
              testID={
                props.idPrefix
                  ? `${props.idPrefix}-${option().value}`
                  : undefined
              }
              selected={active()}
              accessibilityLabel={option().label}
              disabled={props.disabled}
              onPress={() => props.onChange(option().value)}
              style={{
                flex: inline() ? undefined : 1,
                minWidth: 0,
                minHeight:
                  (small() ? control.segmentSmall : control.segment) - 6,
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "center",
                gap: 5,
                paddingHorizontal: inline() ? (platform.desktop ? 9 : 10) : 7,
                borderRadius: platform.desktop ? 6 : 7,
                borderCurve: "continuous",
                backgroundColor: "transparent",
                opacity: props.disabled ? 0.5 : 1,
                ...feedbackStyle(),
              }}
            >
              <Text
                style={{
                  flexShrink: 1,
                  minWidth: 0,
                  numberOfLines: 1,
                  fontSize: platform.desktop
                    ? density.font(12)
                    : density.font(small() ? 13 : 14),
                  lineHeight: density.line(
                    small() || platform.desktop ? 18 : 20,
                  ),
                  fontWeight: "500",
                  color: active() ? palette().text : palette().muted,
                }}
              >
                {option().label}
              </Text>
              <If when={option().count !== undefined}>
                <Text
                  style={{
                    flexShrink: 0,
                    fontSize: typography.xs,
                    lineHeight: density.line(16),
                    fontWeight: "500",
                    fontVariant: ["tabular-nums"],
                    color: active() ? palette().muted : palette().faint,
                  }}
                >
                  {String(option().count)}
                </Text>
              </If>
            </Tab>
          );
        }}
      </Each>
    </NativeTabs>
  );
}
