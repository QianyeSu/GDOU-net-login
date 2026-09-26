import SwiftUI

public struct StatusCardView: View {
    @ObservedObject var manager: AutoReconnectManager
    @ObservedObject var monitor: NetworkMonitor
    @State private var isPulsing = false

    public var body: some View {
        VStack(spacing: 12) {
            HStack {
                HStack(spacing: 8) {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 10, height: 10)
                        .scaleEffect(isPulsing && manager.isOnline ? 1.25 : 1.0)
                        .opacity(isPulsing && manager.isOnline ? 0.75 : 1.0)
                        .animation(
                            manager.isOnline ?
                            Animation.easeInOut(duration: 1.2).repeatForever(autoreverses: true) :
                            .default,
                            value: isPulsing
                        )

                    Text(manager.statusText)
                        .font(.system(size: 15, weight: .bold))
                        .foregroundColor(.primary)
                }

                Spacer()

                if manager.isConnecting {
                    ProgressView()
                        .scaleEffect(0.7)
                } else {
                    Button(action: {
                        Task { await manager.checkStatus() }
                    }) {
                        Image(systemName: "arrow.clockwise")
                            .font(.system(size: 12, weight: .semibold))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .help("重新检测状态")
                }
            }

            Divider()
                .opacity(0.3)

            HStack {
                // Real-time Upload
                HStack(spacing: 5) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundColor(.blue)
                    Text(monitor.currentSpeed.uploadFormatted)
                        .font(.system(size: 12, weight: .medium, design: .monospaced))
                        .foregroundColor(.secondary)
                }

                Spacer()

                // Real-time Download
                HStack(spacing: 5) {
                    Image(systemName: "arrow.down")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundColor(.green)
                    Text(monitor.currentSpeed.downloadFormatted)
                        .font(.system(size: 12, weight: .medium, design: .monospaced))
                        .foregroundColor(.secondary)
                }

                Spacer()

                // Network Interface Tag
                HStack(spacing: 4) {
                    Image(systemName: "network")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(monitor.currentSpeed.primaryInterface)
                        .font(.system(size: 11, weight: .regular))
                        .foregroundColor(.secondary)
                }
            }
        }
        .padding(14)
        .liquidGlassCard(cornerRadius: 14)
        .onAppear {
            isPulsing = true
        }
    }

    private var statusColor: Color {
        if manager.isConnecting {
            return .orange
        } else if manager.isOnline {
            return Color(red: 0.15, green: 0.80, blue: 0.40)
        } else {
            return Color.red.opacity(0.85)
        }
    }
}
