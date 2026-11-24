import { platform, Text, View } from "nsh";
import {
  Checkbox,
  Radio,
  RadioGroup as NativeRadioGroup,
  Switch,
} from "nsh/base";
import { Icon } from "./icons";
import { feedbackStyle, follow, motion } from "./motion";
import { Hint } from "./surface";
import {
  control,
  density,
  focusRing,
  palette,
  radius,
  spacing,
  surfaceFinish,
  typography,
} from "./tokens";

export function Toggle(props: {
  id?: string;
  label: string;
  description?: string;
  value: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
  insetRow?: boolean;
  divider?: boolean;
}) {
  const position = follow(() => (props.value ? 1 : 0), motion.move);
  return (
    <Switch
      testID={props.id}
      focusVisibleStyle={focusRing(props.insetRow ? 0 : 1)}
      accessibilityLabel={props.label}
      checked={props.value}
      disabled={props.disabled}
      onChange={(event) => props.onChange(event.checked)}
      style={{
        flexDirection: "row-reverse",
        alignItems: "center",
        gap: spacing.md,
        minHeight: platform.desktop ? 40 : control.row,
        paddingHorizontal: props.insetRow ? spacing.md : 0,
        paddingVertical: platform.desktop
          ? props.insetRow
            ? density.space(10)
            : 0
          : control.rowPadding,
        borderBottomWidth: props.divider ? 1 : 0,
        borderBottomColor: palette().line,
        opacity: props.disabled ? 0.45 : 1,
      }}
      trackStyle={{
        width: platform.desktop ? 28 : 42,
        height: platform.desktop ? 16 : 24,
        borderRadius: radius.full,
        padding: 1,
        borderWidth: 1,
        borderColor: props.value
          ? surfaceFinish("accent").borderColor
          : surfaceFinish("track").borderColor,
        boxShadow: props.value
          ? surfaceFinish("accent").boxShadow
          : surfaceFinish("track").boxShadow,
        backgroundColor: position.interpolate({
          inputRange: [0, 1],
          outputRange: [palette().track, palette().brandFill],
        }),
      }}
      thumbStyle={{
        width: platform.desktop ? 12 : 20,
        height: platform.desktop ? 12 : 20,
        borderRadius: radius.full,
        borderWidth: 1,
        ...(props.value ? surfaceFinish("knobOn") : surfaceFinish("knob")),
        transform: [
          {
            translateX: position.interpolate({
              inputRange: [0, 1],
              outputRange: [0, platform.desktop ? 12 : 18],
            }),
          },
        ],
      }}
    >
      <View style={{ flex: 1, minWidth: 0, gap: density.space(2) }}>
        <Text
          style={{
            color: palette().text,
            fontSize: typography.sm,
            lineHeight: density.line(20),
            fontWeight: "500",
          }}
        >
          {props.label}
        </Text>
        <If when={props.description}>
          <Hint text={props.description!} />
        </If>
      </View>
    </Switch>
  );
}

export function Check(props: {
  id?: string;
  label: string;
  description?: string;
  value: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
  insetRow?: boolean;
  divider?: boolean;
}) {
  const selected = follow(() => (props.value ? 1 : 0));
  return (
    <Checkbox
      testID={props.id}
      checked={props.value}
      disabled={props.disabled}
      accessibilityLabel={props.label}
      focusVisibleStyle={focusRing(props.insetRow ? 0 : 1)}
      onChange={(event) => props.onChange(event.checked)}
      style={{
        flexDirection: "row",
        alignItems: "center",
        gap: spacing.md,
        minHeight: platform.desktop ? 40 : control.row,
        paddingHorizontal: props.insetRow ? spacing.md : 0,
        paddingVertical: platform.desktop
          ? props.insetRow
            ? density.space(8)
            : 0
          : control.rowPadding,
        borderBottomWidth: props.divider ? 1 : 0,
        borderBottomColor: palette().line,
        opacity: props.disabled ? 0.45 : 1,
      }}
      indicatorStyle={{
        width: control.box,
        height: control.box,
        borderRadius: platform.desktop ? 5 : 7,
        borderCurve: "continuous",
        borderWidth: 1,
        borderColor: props.value
          ? surfaceFinish("accent").borderColor
          : palette().fieldBorder,
        boxShadow: props.value ? surfaceFinish("accent").boxShadow : undefined,
        backgroundColor: props.value ? palette().brandFill : palette().field,
        alignItems: "center",
        justifyContent: "center",
        ...feedbackStyle(),
      }}
      indicator={
        <View style={{ opacity: selected, transform: [{ scale: selected }] }}>
          <Icon
            name="check"
            size={platform.desktop ? 12 : 16}
            strokeWidth={2.6}
            color={palette().primaryText}
          />
        </View>
      }
    >
      <View style={{ flex: 1, minWidth: 0, gap: spacing.xxs }}>
        <Text
          style={{
            color: palette().text,
            fontSize: typography.sm,
            lineHeight: density.line(20),
            fontWeight: "500",
          }}
        >
          {props.label}
        </Text>
        <If when={props.description}>
          <Hint text={props.description!} />
        </If>
      </View>
    </Checkbox>
  );
}

export function RadioMark(props: { selected: boolean }) {
  const scale = follow(() => (props.selected ? 1 : 0));
  const dot = () => (platform.desktop ? 6 : 8);
  return (
    <View
      style={{
        width: control.box,
        height: control.box,
        borderRadius: radius.full,
        borderWidth: 1,
        borderColor: props.selected
          ? surfaceFinish("accent").borderColor
          : palette().fieldBorder,
        boxShadow: props.selected
          ? surfaceFinish("accent").boxShadow
          : undefined,
        backgroundColor: props.selected ? palette().brandFill : palette().field,
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
      }}
    >
      <View
        style={{
          width: dot(),
          height: dot(),
          borderRadius: radius.full,
          backgroundColor: palette().primaryText,
          opacity: scale,
          transform: [{ scale }],
        }}
      />
    </View>
  );
}

export function RadioGroup(props: {
  idPrefix?: string;
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  insetRow?: boolean;
}) {
  return (
    <NativeRadioGroup style={{ gap: props.insetRow ? 0 : spacing.xxs }}>
      <Each of={props.options} key={(option) => option.value}>
        {(option, index) => (
          <RadioChoice
            id={
              props.idPrefix
                ? `${props.idPrefix}-${option().value}`
                : undefined
            }
            label={option().label}
            selected={props.value === option().value}
            insetRow={props.insetRow}
            divider={props.insetRow && index() < props.options.length - 1}
            onSelect={() => {
              if (props.value !== option().value) {
                props.onChange(option().value);
              }
            }}
          />
        )}
      </Each>
    </NativeRadioGroup>
  );
}

function RadioChoice(props: {
  id?: string;
  label: string;
  selected: boolean;
  insetRow?: boolean;
  divider?: boolean;
  onSelect: () => void;
}) {
  return (
    <Radio
      testID={props.id}
      focusVisibleStyle={focusRing(props.insetRow ? 0 : 1)}
      accessibilityLabel={props.label}
      checked={props.selected}
      onChange={props.onSelect}
      hoverStyle={{ backgroundColor: palette().hover }}
      activeStyle={{ backgroundColor: palette().soft }}
      style={{
        minHeight: platform.desktop ? 40 : control.row,
        flexDirection: "row",
        alignItems: "center",
        gap: spacing.md,
        paddingVertical: platform.desktop
          ? density.space(8)
          : control.rowPadding,
        paddingHorizontal: spacing.md,
        borderRadius: props.insetRow ? 0 : radius.md,
        borderCurve: props.insetRow ? undefined : "continuous",
        borderBottomWidth: props.divider ? 1 : 0,
        borderBottomColor: palette().line,
        backgroundColor: props.insetRow
          ? "transparent"
          : props.selected
            ? palette().soft
            : "transparent",
        ...feedbackStyle(),
      }}
    >
      <RadioMark selected={props.selected} />
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: typography.sm,
          lineHeight: density.line(20),
          fontWeight: props.selected ? "600" : "500",
          color: palette().text,
        }}
      >
        {props.label}
      </Text>
    </Radio>
  );
}
