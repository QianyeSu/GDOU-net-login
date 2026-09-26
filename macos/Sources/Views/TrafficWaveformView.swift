import SwiftUI

public struct TrafficWaveformView: View {
    @ObservedObject var monitor: NetworkMonitor
    @Environment(\.colorScheme) var colorScheme
    @State private var hoveredIndex: Int?
    @State private var hoverY: CGFloat?

    private let downloadColor = Color(red: 0.15, green: 0.80, blue: 0.40)
    private let uploadColor = Color(red: 0.96, green: 0.35, blue: 0.35)

    public var body: some View {
        VStack(spacing: 8) {
            // Header: "流量" and Legend
            HStack {
                Text("流量")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundColor(.secondary)

                Spacer()

                HStack(spacing: 12) {
                    legendItem(color: downloadColor, title: "下载")
                    legendItem(color: uploadColor, title: "上传")
                }
            }

            // Real-time Canvas Waveform Chart.  The continuous hover target
            // mirrors the Windows chart: moving the pointer over any sample
            // shows a timestamp plus both upload/download values.
            GeometryReader { geo in
                let width = geo.size.width
                let height = geo.size.height
                let maxVal = max(
                    max(monitor.downloadHistory.max() ?? 0, monitor.uploadHistory.max() ?? 0),
                    1024
                )
                let pointCount = max(monitor.speedHistory.count, 2)

                ZStack {
                    // Dotted horizontal grid lines
                    Path { path in
                        path.move(to: CGPoint(x: 0, y: height * 0.33))
                        path.addLine(to: CGPoint(x: width, y: height * 0.33))
                        path.move(to: CGPoint(x: 0, y: height * 0.66))
                        path.addLine(to: CGPoint(x: width, y: height * 0.66))
                    }
                    .stroke(
                        Color.primary.opacity(0.06),
                        style: StrokeStyle(lineWidth: 1, dash: [4, 4])
                    )

                    renderWave(
                        data: monitor.downloadHistory,
                        width: width,
                        height: height,
                        maxVal: maxVal,
                        strokeColor: downloadColor,
                        fillColor: downloadColor.opacity(0.18)
                    )

                    renderWave(
                        data: monitor.uploadHistory,
                        width: width,
                        height: height,
                        maxVal: maxVal,
                        strokeColor: uploadColor,
                        fillColor: uploadColor.opacity(0.18)
                    )

                    if let hoveredIndex,
                       hoveredIndex < pointCount,
                       let sample = sample(at: hoveredIndex),
                       let x = xPosition(for: hoveredIndex, count: pointCount, width: width) {
                        Path { path in
                            path.move(to: CGPoint(x: x, y: 0))
                            path.addLine(to: CGPoint(x: x, y: height))
                        }
                        .stroke(Color.primary.opacity(0.28), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))

                        Circle()
                            .fill(downloadColor)
                            .frame(width: 7, height: 7)
                            .overlay(Circle().stroke(Color.white, lineWidth: 1.5))
                            .position(point(
                                value: Double(sample.downloadBytesPerSec),
                                index: hoveredIndex,
                                count: pointCount,
                                width: width,
                                height: height,
                                maxVal: maxVal
                            ))

                        Circle()
                            .fill(uploadColor)
                            .frame(width: 7, height: 7)
                            .overlay(Circle().stroke(Color.white, lineWidth: 1.5))
                            .position(point(
                                value: Double(sample.uploadBytesPerSec),
                                index: hoveredIndex,
                                count: pointCount,
                                width: width,
                                height: height,
                                maxVal: maxVal
                            ))
                    }
                }
                .overlay(alignment: .topLeading) {
                    if let hoveredIndex,
                       let sample = sample(at: hoveredIndex) {
                        hoverTooltip(sample: sample)
                            .frame(width: 196)
                            // Keep the Rust/Windows one-row readout, but keep
                            // it inside the plot so ScrollView cannot clip it.
                            .offset(
                                x: tooltipX(for: hoveredIndex, count: pointCount, width: width),
                                y: tooltipY(for: hoverY, height: height)
                            )
                            .zIndex(10)
                    }
                }
                .contentShape(Rectangle())
                .onContinuousHover(coordinateSpace: .local) { phase in
                    switch phase {
                    case .active(let location):
                        let count = max(monitor.speedHistory.count, 2)
                        let raw = location.x / max(width, 1) * CGFloat(count - 1)
                        hoveredIndex = min(max(Int(raw.rounded()), 0), count - 1)
                        hoverY = location.y
                    case .ended:
                        hoveredIndex = nil
                        hoverY = nil
                    @unknown default:
                        hoveredIndex = nil
                        hoverY = nil
                    }
                }
            }
            .frame(height: 64)
            .background(Color.primary.opacity(0.02))
            .cornerRadius(8)

            // Speed Indicators (Download & Upload side by side)
            HStack(spacing: 8) {
                speedCard(
                    icon: "arrow.down",
                    color: downloadColor,
                    value: monitor.currentSpeed.downloadFormatted,
                    title: "下载速率"
                )
                speedCard(
                    icon: "arrow.up",
                    color: uploadColor,
                    value: monitor.currentSpeed.uploadFormatted,
                    title: "上传速率"
                )
            }
        }
        .padding(12)
        .background(colorScheme == .dark ? Color.white.opacity(0.03) : Color.white)
        .cornerRadius(12)
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.primary.opacity(0.08), lineWidth: 1)
        )
    }

    private func legendItem(color: Color, title: String) -> some View {
        HStack(spacing: 4) {
            Circle()
                .fill(color)
                .frame(width: 6, height: 6)
            Text(title)
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(.secondary)
        }
    }

    private func speedCard(icon: String, color: Color, value: String, title: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 13, weight: .bold))
                .foregroundColor(color)
                .frame(width: 28, height: 28)
                .background(color.opacity(0.12))
                .cornerRadius(6)

            VStack(alignment: .leading, spacing: 1) {
                Text(value)
                    .font(.system(size: 13, weight: .bold, design: .monospaced))
                    .foregroundColor(.primary)
                Text(title)
                    .font(.system(size: 10))
                    .foregroundColor(.secondary)
            }
            Spacer()
        }
        .padding(8)
        .background(colorScheme == .dark ? Color.white.opacity(0.04) : Color.white)
        .cornerRadius(8)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.primary.opacity(0.07), lineWidth: 1))
    }

    @ViewBuilder
    private func renderWave(data: [Double], width: CGFloat, height: CGFloat, maxVal: Double, strokeColor: Color, fillColor: Color) -> some View {
        let stepX = width / CGFloat(max(data.count - 1, 1))
        let points: [CGPoint] = data.enumerated().map { index, value in
            let x = CGFloat(index) * stepX
            let normalized = min(max(value / maxVal, 0.0), 1.0)
            let y = height - (CGFloat(normalized) * (height - 8) + 4)
            return CGPoint(x: x, y: y)
        }

        ZStack {
            Path { path in
                guard let first = points.first else { return }
                path.move(to: CGPoint(x: first.x, y: height))
                path.addLine(to: first)
                for point in points.dropFirst() {
                    path.addLine(to: point)
                }
                if let last = points.last {
                    path.addLine(to: CGPoint(x: last.x, y: height))
                }
                path.closeSubpath()
            }
            .fill(
                LinearGradient(
                    colors: [fillColor, fillColor.opacity(0.0)],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )

            Path { path in
                guard let first = points.first else { return }
                path.move(to: first)
                for point in points.dropFirst() {
                    path.addLine(to: point)
                }
            }
            .stroke(strokeColor, style: StrokeStyle(lineWidth: 1.8, lineCap: .round, lineJoin: .round))
        }
    }

    private func sample(at index: Int) -> NetworkSpeedHistoryPoint? {
        guard monitor.speedHistory.indices.contains(index) else { return nil }
        return monitor.speedHistory[index]
    }

    private func xPosition(for index: Int, count: Int, width: CGFloat) -> CGFloat? {
        guard count > 1 else { return nil }
        return CGFloat(index) / CGFloat(count - 1) * width
    }

    private func point(value: Double, index: Int, count: Int, width: CGFloat, height: CGFloat, maxVal: Double) -> CGPoint {
        let x = xPosition(for: index, count: count, width: width) ?? 0
        let normalized = min(max(value / maxVal, 0), 1)
        let y = height - (CGFloat(normalized) * (height - 8) + 4)
        return CGPoint(x: x, y: y)
    }

    private func tooltipX(for index: Int, count: Int, width: CGFloat) -> CGFloat {
        let tooltipWidth: CGFloat = 196
        let x = xPosition(for: index, count: count, width: width) ?? 0
        return min(max(x - tooltipWidth / 2, 4), max(width - tooltipWidth - 4, 4))
    }

    private func tooltipY(for pointerY: CGFloat?, height: CGFloat) -> CGFloat {
        let tooltipHeight: CGFloat = 28
        let preferred = (pointerY ?? 8) - tooltipHeight - 6
        return min(max(preferred, 3), max(height - tooltipHeight - 3, 3))
    }

    private func hoverTooltip(sample: NetworkSpeedHistoryPoint) -> some View {
        HStack(spacing: 6) {
            Text(Self.tooltipDateFormatter.string(from: sample.timestamp))
                .font(.system(size: 9, weight: .semibold, design: .monospaced))
                .foregroundStyle(.primary)

            inlineTooltipValue(color: downloadColor, value: sample.downloadBytesPerSec)
            inlineTooltipValue(color: uploadColor, value: sample.uploadBytesPerSec)
        }
        .font(.system(size: 9, weight: .medium, design: .monospaced))
        .lineLimit(1)
        .minimumScaleFactor(0.7)
        .padding(.horizontal, 7)
        .padding(.vertical, 5)
        .background(.regularMaterial)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.primary.opacity(0.12), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .shadow(color: .black.opacity(0.14), radius: 6, y: 2)
    }

    private func inlineTooltipValue(color: Color, value: UInt64) -> some View {
        HStack(spacing: 5) {
            Circle().fill(color).frame(width: 6, height: 6)
            Text(NetworkSpeedSnapshot.formatBytesPerSec(value))
                .font(.system(size: 9, weight: .semibold, design: .monospaced))
        }
    }

    private static let tooltipDateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "zh_CN")
        formatter.dateFormat = "HH:mm:ss"
        return formatter
    }()
}
