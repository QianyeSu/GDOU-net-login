import SwiftUI

public struct TrafficWaveformView: View {
    @ObservedObject var monitor: NetworkMonitor
    @Environment(\.colorScheme) var colorScheme

    public var body: some View {
        VStack(spacing: 8) {
            // Header: "流量" and Legend
            HStack {
                Text("流量")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundColor(.secondary)

                Spacer()

                HStack(spacing: 12) {
                    HStack(spacing: 4) {
                        Circle()
                            .fill(Color(red: 0.15, green: 0.75, blue: 0.38))
                            .frame(width: 6, height: 6)
                        Text("下载")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundColor(.secondary)
                    }

                    HStack(spacing: 4) {
                        Circle()
                            .fill(Color(red: 0.95, green: 0.30, blue: 0.30))
                            .frame(width: 6, height: 6)
                        Text("上传")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundColor(.secondary)
                    }
                }
            }

            // Real-time Canvas Waveform Chart
            GeometryReader { geo in
                let width = geo.size.width
                let height = geo.size.height

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

                    // Download Waveform (Green)
                    renderWave(
                        data: monitor.downloadHistory,
                        width: width,
                        height: height,
                        strokeColor: Color(red: 0.15, green: 0.80, blue: 0.40),
                        fillColor: Color(red: 0.15, green: 0.80, blue: 0.40).opacity(0.18)
                    )

                    // Upload Waveform (Red)
                    renderWave(
                        data: monitor.uploadHistory,
                        width: width,
                        height: height,
                        strokeColor: Color(red: 0.96, green: 0.35, blue: 0.35),
                        fillColor: Color(red: 0.96, green: 0.35, blue: 0.35).opacity(0.18)
                    )
                }
            }
            .frame(height: 64)
            .background(Color.primary.opacity(0.02))
            .cornerRadius(8)

            // Speed Indicators (Download & Upload side by side)
            HStack(spacing: 8) {
                // Download Box
                HStack(spacing: 8) {
                    Image(systemName: "arrow.down")
                        .font(.system(size: 13, weight: .bold))
                        .foregroundColor(Color(red: 0.15, green: 0.75, blue: 0.38))
                        .frame(width: 28, height: 28)
                        .background(Color(red: 0.15, green: 0.75, blue: 0.38).opacity(0.12))
                        .cornerRadius(6)

                    VStack(alignment: .leading, spacing: 1) {
                        Text(monitor.currentSpeed.downloadFormatted)
                            .font(.system(size: 13, weight: .bold, design: .monospaced))
                            .foregroundColor(.primary)
                        Text("下载速率")
                            .font(.system(size: 10))
                            .foregroundColor(.secondary)
                    }
                    Spacer()
                }
                .padding(8)
                .background(colorScheme == .dark ? Color.white.opacity(0.04) : Color.white)
                .cornerRadius(8)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.primary.opacity(0.07), lineWidth: 1))

                // Upload Box
                HStack(spacing: 8) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 13, weight: .bold))
                        .foregroundColor(Color(red: 0.95, green: 0.30, blue: 0.30))
                        .frame(width: 28, height: 28)
                        .background(Color(red: 0.95, green: 0.30, blue: 0.30).opacity(0.12))
                        .cornerRadius(6)

                    VStack(alignment: .leading, spacing: 1) {
                        Text(monitor.currentSpeed.uploadFormatted)
                            .font(.system(size: 13, weight: .bold, design: .monospaced))
                            .foregroundColor(.primary)
                        Text("上传速率")
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
        }
        .padding(12)
        .background(colorScheme == .dark ? Color.white.opacity(0.03) : Color.white)
        .cornerRadius(12)
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.primary.opacity(0.08), lineWidth: 1)
        )
    }

    @ViewBuilder
    private func renderWave(data: [Double], width: CGFloat, height: CGFloat, strokeColor: Color, fillColor: Color) -> some View {
        let maxVal = max(data.max() ?? 1024, 1024)
        let stepX = width / CGFloat(max(data.count - 1, 1))

        let points: [CGPoint] = data.enumerated().map { (idx, val) in
            let x = CGFloat(idx) * stepX
            let normalized = min(max(val / maxVal, 0.0), 1.0)
            let y = height - (CGFloat(normalized) * (height - 8) + 4)
            return CGPoint(x: x, y: y)
        }

        ZStack {
            // Area fill
            Path { path in
                guard let first = points.first else { return }
                path.move(to: CGPoint(x: first.x, y: height))
                path.addLine(to: first)
                for pt in points.dropFirst() {
                    path.addLine(to: pt)
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

            // Stroke line
            Path { path in
                guard let first = points.first else { return }
                path.move(to: first)
                for pt in points.dropFirst() {
                    path.addLine(to: pt)
                }
            }
            .stroke(strokeColor, style: StrokeStyle(lineWidth: 1.8, lineCap: .round, lineJoin: .round))
        }
    }
}
