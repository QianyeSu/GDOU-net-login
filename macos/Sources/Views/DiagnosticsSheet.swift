import SwiftUI

public struct DiagnosticsSheet: View {
    @ObservedObject var manager: AutoReconnectManager
    @ObservedObject var monitor: NetworkMonitor
    @Environment(\.dismiss) var dismiss

    @State private var isTesting = false
    @State private var challengeResult: String?
    @State private var traces: [PortalProbeTrace] = []

    public var body: some View {
        VStack(spacing: 16) {
            // Header
            HStack {
                HStack(spacing: 8) {
                    Image(systemName: "stethoscope")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundColor(.blue)
                    Text("网络与 Portal 诊断")
                        .font(.system(size: 16, weight: .bold))
                }
                Spacer()
                Button("关闭") {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
            }
            .padding(.bottom, 4)

            ScrollView {
                VStack(spacing: 14) {
                    // Actions
                    HStack(spacing: 10) {
                        Button(action: runDiagnosis) {
                            HStack(spacing: 6) {
                                if isTesting {
                                    ProgressView().scaleEffect(0.6)
                                } else {
                                    Image(systemName: "waveform.path.ecg")
                                }
                                Text("运行全链路体检")
                            }
                            .frame(maxWidth: .infinity)
                            .frame(height: 32)
                            .background(Color.blue.opacity(0.15))
                            .foregroundColor(.blue)
                            .cornerRadius(8)
                        }
                        .buttonStyle(.plain)
                        .disabled(isTesting)

                        Button(action: {
                            monitor.refreshInterfaces()
                        }) {
                            HStack(spacing: 6) {
                                Image(systemName: "arrow.clockwise")
                                Text("刷新网卡")
                            }
                            .frame(maxWidth: .infinity)
                            .frame(height: 32)
                            .background(Color.primary.opacity(0.06))
                            .cornerRadius(8)
                        }
                        .buttonStyle(.plain)
                    }

                    // Challenge Diagnostic Card
                    if let challengeResult = challengeResult {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("Challenge 状态")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundColor(.secondary)
                            Text(challengeResult)
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundColor(.primary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .liquidGlassCard(cornerRadius: 10)
                    }

                    // Probes list
                    if !traces.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Portal 探测轨迹")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundColor(.secondary)

                            ForEach(traces) { trace in
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(trace.target)
                                            .font(.system(size: 12, weight: .medium))
                                            .lineLimit(1)
                                        if let location = trace.location {
                                            Text("重定向 -> \(location)")
                                                .font(.system(size: 11))
                                                .foregroundColor(.blue)
                                                .lineLimit(1)
                                        }
                                    }
                                    Spacer()
                                    if let status = trace.status {
                                        Text("HTTP \(status)")
                                            .font(.system(size: 10, weight: .semibold, design: .monospaced))
                                            .padding(.horizontal, 6)
                                            .padding(.vertical, 2)
                                            .background(status == 200 || status == 302 ? Color.green.opacity(0.2) : Color.orange.opacity(0.2))
                                            .cornerRadius(4)
                                    }
                                }
                                .padding(8)
                                .background(Color.primary.opacity(0.03))
                                .cornerRadius(6)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .liquidGlassCard(cornerRadius: 10)
                    }

                    // Network Interfaces List
                    VStack(alignment: .leading, spacing: 8) {
                        Text("本机网络接口")
                            .font(.system(size: 12, weight: .bold))
                            .foregroundColor(.secondary)

                        ForEach(monitor.interfaces) { iface in
                            HStack {
                                Image(systemName: iface.isWifi ? "wifi" : "cable.connector")
                                    .foregroundColor(iface.isUp ? .green : .secondary)

                                VStack(alignment: .leading, spacing: 2) {
                                    Text(iface.displayName)
                                        .font(.system(size: 12, weight: .medium))
                                    Text(iface.ip)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundColor(.secondary)
                                }
                                Spacer()
                                Text(iface.isUp ? "活动" : "休眠")
                                    .font(.system(size: 10))
                                    .foregroundColor(iface.isUp ? .green : .secondary)
                            }
                            .padding(8)
                            .background(Color.primary.opacity(0.03))
                            .cornerRadius(6)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12)
                    .liquidGlassCard(cornerRadius: 10)
                }
            }
        }
        .padding(20)
        .frame(width: 440, height: 480)
        .background(.ultraThinMaterial)
    }

    private func runDiagnosis() {
        isTesting = true
        Task {
            let client = SrunClient(config: manager.config)
            traces = await client.probeDetailedTraces()

            do {
                let detected = try await client.probePortalFast()
                let portal = detected.portalUrl ?? (manager.config.portalUrl.isEmpty ? "http://172.16.200.11" : manager.config.portalUrl)
                let acid = detected.acid ?? manager.config.acid ?? 1
                let ip = detected.userIp
                    ?? manager.config.bindIp
                    ?? manager.config.userIp
                    ?? monitor.interfaces.first(where: { $0.isWifi || $0.name.hasPrefix("en") })?.ip
                    ?? "0.0.0.0"
                let token = try await client.getChallenge(portalUrl: portal, ip: ip, acid: acid)
                challengeResult = "握手成功: Portal=\(portal), ac_id=\(acid), Token=\(token.prefix(8))..."
            } catch {
                challengeResult = "握手失败: \(error.localizedDescription)"
            }
            isTesting = false
        }
    }
}
