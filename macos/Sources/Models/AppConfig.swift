import Foundation

public struct AppConfig: Codable, Equatable {
    public var portalUrl: String
    public var probeUrl: String
    public var username: String
    public var acid: UInt32?
    public var userIp: String?
    public var bindIp: String?
    /// Whether the user explicitly selected `bindIp` in the advanced network
    /// picker.  `nil`/false keeps legacy configs in automatic mode, so an old
    /// DHCP address is not accidentally submitted after moving between APs.
    public var bindIpExplicit: Bool?
    public var retrySeconds: UInt64
    public var onlineCheckSeconds: UInt64
    public var autoQueryAcid: Bool
    public var autoReconnect: Bool
    public var startupEnabled: Bool
    public var rememberPassword: Bool
    public var osName: String
    public var deviceName: String
    public var n: UInt32
    public var loginType: UInt32

    public static let defaultProbeUrl = "http://www.msftconnecttest.com/connecttest.txt"

    public static var `default`: AppConfig {
        AppConfig(
            portalUrl: "",
            probeUrl: defaultProbeUrl,
            username: "",
            acid: nil,
            userIp: nil,
            bindIp: nil,
            bindIpExplicit: false,
            retrySeconds: 15,
            onlineCheckSeconds: 60,
            autoQueryAcid: true,
            autoReconnect: true,
            startupEnabled: false,
            rememberPassword: true,
            // Match the Rust/Windows client's `std::env::consts::OS` value;
            // some SRUN deployments use this field for client statistics.
            osName: "macos",
            deviceName: Host.current().localizedName ?? "MacBook",
            n: 200,
            loginType: 1
        )
    }

    private enum CodingKeys: String, CodingKey {
        case portalUrl = "portal_url"
        case probeUrl = "probe_url"
        case username
        case acid = "ac_id"
        case userIp = "user_ip"
        case bindIp = "bind_ip"
        case bindIpExplicit = "bind_ip_explicit"
        case retrySeconds = "retry_seconds"
        case onlineCheckSeconds = "online_check_seconds"
        case autoQueryAcid = "auto_query_acid"
        case autoReconnect = "auto_reconnect"
        case startupEnabled = "startup_enabled"
        case rememberPassword = "remember_password"
        case osName = "os_name"
        case deviceName = "device_name"
        case n
        case loginType = "login_type"
    }

    public static var configDirectory: URL {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("cn.gdou.gdou-net-login", isDirectory: true)
    }

    public static var configFileURL: URL {
        configDirectory.appendingPathComponent("config.json")
    }

    public static func load() -> AppConfig {
        let fileURL = configFileURL
        guard let data = try? Data(contentsOf: fileURL),
              var decoded = try? JSONDecoder().decode(AppConfig.self, from: data) else {
            return .default
        }
        if decoded.osName.caseInsensitiveCompare("macOS") == .orderedSame {
            decoded.osName = "macos"
        }
        return decoded
    }

    public func save() {
        let dir = Self.configDirectory
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let fileURL = Self.configFileURL
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        if let data = try? encoder.encode(self) {
            try? data.write(to: fileURL, options: .atomic)
        }
    }
}
