import Foundation

public struct NetworkInterfaceItem: Identifiable, Hashable {
    public var id: String { name + "_" + ip }
    public let name: String
    public let displayName: String
    public let ip: String
    public let netmask: String?
    public let isUp: Bool
    public let isLoopback: Bool
    public let isWifi: Bool

    public init(name: String, displayName: String, ip: String, netmask: String? = nil, isUp: Bool = true, isLoopback: Bool = false, isWifi: Bool = false) {
        self.name = name
        self.displayName = displayName
        self.ip = ip
        self.netmask = netmask
        self.isUp = isUp
        self.isLoopback = isLoopback
        self.isWifi = isWifi
    }
}

public struct NetworkSpeedSnapshot: Equatable {
    public let uploadBytesPerSec: UInt64
    public let downloadBytesPerSec: UInt64
    public let totalUploadBytes: UInt64
    public let totalDownloadBytes: UInt64
    public let primaryInterface: String

    public var uploadFormatted: String {
        Self.formatBytesPerSec(uploadBytesPerSec)
    }

    public var downloadFormatted: String {
        Self.formatBytesPerSec(downloadBytesPerSec)
    }

    public static func formatBytesPerSec(_ bytes: UInt64) -> String {
        if bytes < 1024 {
            return "\(bytes) B/s"
        } else if bytes < 1024 * 1024 {
            return String(format: "%.1f KB/s", Double(bytes) / 1024.0)
        } else {
            return String(format: "%.1f MB/s", Double(bytes) / (1024.0 * 1024.0))
        }
    }

    public static var zero: NetworkSpeedSnapshot {
        NetworkSpeedSnapshot(
            uploadBytesPerSec: 0,
            downloadBytesPerSec: 0,
            totalUploadBytes: 0,
            totalDownloadBytes: 0,
            primaryInterface: "en0"
        )
    }
}

/// One sample in the rolling waveform history.  Keeping the timestamp next
/// to the values lets the macOS chart provide the same hover readout as the
/// Windows chart instead of only showing the current speed cards.
public struct NetworkSpeedHistoryPoint: Identifiable, Equatable {
    public let timestamp: Date
    public let uploadBytesPerSec: UInt64
    public let downloadBytesPerSec: UInt64

    public var id: Date { timestamp }

    public init(timestamp: Date = Date(), uploadBytesPerSec: UInt64 = 0, downloadBytesPerSec: UInt64 = 0) {
        self.timestamp = timestamp
        self.uploadBytesPerSec = uploadBytesPerSec
        self.downloadBytesPerSec = downloadBytesPerSec
    }
}
