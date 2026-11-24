import { Canvas, Path2D, Text, View } from "nsh";
import { formatRate } from "../locale";
import type { TrafficSample } from "../model";
import { density, palette, radius, spacing, typography } from "./tokens";

export function TrafficChart(props: {
  samples: TrafficSample[];
  height?: number;
  downloadLabel: string;
  uploadLabel: string;
}) {
  const height = props.height ?? 140;
  const samples = () => props.samples;
  const maxRate = () => {
    let peak = 1024;
    for (const s of samples()) {
      if (s.rxBytesPerSec > peak) peak = s.rxBytesPerSec;
      if (s.txBytesPerSec > peak) peak = s.txBytesPerSec;
    }
    return peak * 1.15;
  };

  return (
    <View style={{ gap: spacing.sm }}>
      <View
        style={{
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: spacing.sm,
        }}
      >
        <View style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
          <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
            <View
              style={{
                width: 8,
                height: 8,
                borderRadius: radius.full,
                backgroundColor: palette().chartDownload,
              }}
            />
            <Text style={{ fontSize: typography.note, color: palette().muted }}>
              {props.downloadLabel}
            </Text>
          </View>
          <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
            <View
              style={{
                width: 8,
                height: 8,
                borderRadius: radius.full,
                backgroundColor: palette().chartUpload,
              }}
            />
            <Text style={{ fontSize: typography.note, color: palette().muted }}>
              {props.uploadLabel}
            </Text>
          </View>
        </View>
        <Text
          style={{
            fontSize: typography.xs,
            color: palette().faint,
            fontVariant: ["tabular-nums"],
          }}
        >
          {formatRate(maxRate())}
        </Text>
      </View>
      <Canvas
        style={{
          width: "100%",
          height,
          borderRadius: radius.md,
        }}
        draw={(ctx, size) => {
          const w = size.width;
          const h = size.height;
          if (w <= 0 || h <= 0) return;

          const padTop = 8;
          const padBottom = 6;
          const plotH = Math.max(1, h - padTop - padBottom);

          // Horizontal reference gridlines.
          ctx.strokeStyle = palette().line;
          ctx.lineWidth = 1;
          for (let i = 0; i <= 2; i++) {
            const y = padTop + (plotH * i) / 2;
            const grid = new Path2D();
            grid.moveTo(0, y);
            grid.lineTo(w, y);
            ctx.stroke(grid);
          }

          const list = samples();
          if (list.length === 0) return;
          const slots = Math.max(2, list.length);
          const peak = maxRate();

          const drawSeries = (
            selector: (sample: TrafficSample) => number,
            color: string,
            fillAlpha: number,
          ) => {
            const strokePath = new Path2D();
            const fillPath = new Path2D();
            for (let i = 0; i < list.length; i++) {
              const x = list.length === 1 ? w : (i / (slots - 1)) * w;
              const ratio = Math.min(1, Math.max(0, selector(list[i]) / peak));
              const y = padTop + plotH * (1 - ratio);
              if (i === 0) {
                strokePath.moveTo(x, y);
                fillPath.moveTo(x, padTop + plotH);
                fillPath.lineTo(x, y);
              } else {
                strokePath.lineTo(x, y);
                fillPath.lineTo(x, y);
              }
            }
            const lastX = list.length === 1 ? w : ((list.length - 1) / (slots - 1)) * w;
            fillPath.lineTo(lastX, padTop + plotH);
            fillPath.closePath();

            ctx.save();
            ctx.globalAlpha = fillAlpha;
            ctx.fillStyle = color;
            ctx.fill(fillPath);
            ctx.restore();

            ctx.strokeStyle = color;
            ctx.lineWidth = 2;
            ctx.lineCap = "round";
            ctx.lineJoin = "round";
            ctx.stroke(strokePath);
          };

          drawSeries((s) => s.rxBytesPerSec, palette().chartDownload, 0.16);
          drawSeries((s) => s.txBytesPerSec, palette().chartUpload, 0.12);
        }}
      />
    </View>
  );
}
