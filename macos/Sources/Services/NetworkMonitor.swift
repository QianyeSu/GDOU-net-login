import Foundation
import Darwin

public final class NetworkMonitor: ObservableObject {
    @Published public private(set) var currentSpeed: NetworkSpeedSnapshot = .zero
    @Published public private(set) var interfaces: [NetworkInterfaceItem] = []
    @Published public private(set) var uploadHistory: [Double] = Array(repeating: 0.0, count: 32)
    @Published public private(set) var downloadHistory: [Double] = Array(repeating: 0.0, count: 32)
    @Published public private(set) var totalTrafficBytes: UInt64 = 0
    @Published public private(set) var onlineDurationSeconds: Int = 0

    public var isOnline: Bool = false {
        didSet {
            if !isOnline {
                onlineDurationSeconds = 0
            }
        }
    }

    private var timer: Timer?
    private var lastBytesIn: UInt64 = 0
    private var lastBytesOut: UInt64 = 0
    private var lastTimestamp: Date = Date()
    private var hasBaseline: Bool = false

    public init() {
        refreshInterfaces()
    }

    public func startMonitoring() {
        stopMonitoring()
        hasBaseline = false
        sample()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.sample()
        }
    }

    public func stopMonitoring() {
        timer?.invalidate()
        timer = nil
    }

    public func refreshInterfaces() {
        var items: [NetworkInterfaceItem] = []
        var ifap: UnsafeMutablePointer<ifaddrs>?

        guard getifaddrs(&ifap) == 0, let first = ifap else { return }
        defer { freeifaddrs(ifap) }

        var ptr: UnsafeMutablePointer<ifaddrs>? = first
        while let current = ptr {
            let interface = current.pointee
            let flags = Int32(interface.ifa_flags)
            let isUp = (flags & IFF_UP) != 0
            let isLoopback = (flags & IFF_LOOPBACK) != 0

            if let addr = interface.ifa_addr, addr.pointee.sa_family == UInt8(AF_INET) {
                var hostname = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                let saLen = socklen_t(addr.pointee.sa_len)
                if getnameinfo(addr, saLen, &hostname, socklen_t(hostname.count), nil, 0, NI_NUMERICHOST) == 0 {
                    let ip = String(cString: hostname)
                    let name = String(cString: interface.ifa_name)
                    let isWifi = name.starts(with: "en") && (name == "en0" || name == "en1")
                    let displayName = isWifi ? "Wi-Fi (\(name))" : name

                    let item = NetworkInterfaceItem(
                        name: name,
                        displayName: displayName,
                        ip: ip,
                        isUp: isUp,
                        isLoopback: isLoopback,
                        isWifi: isWifi
                    )
                    if !isLoopback && !items.contains(where: { $0.ip == ip }) {
                        items.append(item)
                    }
                }
            }
            ptr = interface.ifa_next
        }

        DispatchQueue.main.async {
            self.interfaces = items
        }
    }

    private func sample() {
        var totalIn: UInt64 = 0
        var totalOut: UInt64 = 0
        var primaryInterface = "en0"

        var ifap: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&ifap) == 0, let first = ifap else { return }
        defer { freeifaddrs(ifap) }

        var ptr: UnsafeMutablePointer<ifaddrs>? = first
        while let current = ptr {
            let interface = current.pointee
            let flags = Int32(interface.ifa_flags)
            let isLoopback = (flags & IFF_LOOPBACK) != 0

            if !isLoopback, let data = interface.ifa_data, interface.ifa_addr?.pointee.sa_family == UInt8(AF_LINK) {
                let networkData = data.assumingMemoryBound(to: if_data.self).pointee
                totalIn += UInt64(networkData.ifi_ibytes)
                totalOut += UInt64(networkData.ifi_obytes)
                let name = String(cString: interface.ifa_name)
                if name.starts(with: "en") {
                    primaryInterface = name
                }
            }
            ptr = interface.ifa_next
        }

        let now = Date()
        let elapsed = max(now.timeIntervalSince(lastTimestamp), 0.5)

        if hasBaseline {
            let deltaIn = totalIn >= lastBytesIn ? totalIn - lastBytesIn : 0
            let deltaOut = totalOut >= lastBytesOut ? totalOut - lastBytesOut : 0

            let speedIn = UInt64(Double(deltaIn) / elapsed)
            let speedOut = UInt64(Double(deltaOut) / elapsed)

            let snapshot = NetworkSpeedSnapshot(
                uploadBytesPerSec: speedOut,
                downloadBytesPerSec: speedIn,
                totalUploadBytes: totalOut,
                totalDownloadBytes: totalIn,
                primaryInterface: primaryInterface
            )

            let speedInDouble = Double(speedIn)
            let speedOutDouble = Double(speedOut)
            let currentTotal = totalIn + totalOut

            DispatchQueue.main.async {
                self.currentSpeed = snapshot
                self.totalTrafficBytes = currentTotal

                if self.isOnline {
                    self.onlineDurationSeconds += 1
                }

                self.uploadHistory.removeFirst()
                self.uploadHistory.append(speedOutDouble)

                self.downloadHistory.removeFirst()
                self.downloadHistory.append(speedInDouble)
            }
        }

        lastBytesIn = totalIn
        lastBytesOut = totalOut
        lastTimestamp = now
        hasBaseline = true
    }

    public var formattedDuration: String {
        let hrs = onlineDurationSeconds / 3600
        let mins = (onlineDurationSeconds % 3600) / 60
        let secs = onlineDurationSeconds % 60
        if hrs > 0 {
            return String(format: "%d:%02d:%02d", hrs, mins, secs)
        } else {
            return String(format: "%d:%02d", mins, secs)
        }
    }

    public var formattedTotalTraffic: String {
        let bytes = Double(totalTrafficBytes)
        if bytes < 1024 * 1024 {
            return String(format: "%.2f KB", bytes / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            return String(format: "%.2f MB", bytes / (1024.0 * 1024.0))
        } else {
            return String(format: "%.2f GB", bytes / (1024.0 * 1024.0 * 1024.0))
        }
    }

    deinit {
        stopMonitoring()
    }
}
