import Foundation

public enum PortalOnlineStatus: String, Codable, Equatable {
    case online
    case offline
    case unknown
}

public struct OnlineStatusAssessment: Equatable {
    public let status: PortalOnlineStatus
    public let detail: String?

    public init(status: PortalOnlineStatus, detail: String? = nil) {
        self.status = status
        self.detail = detail
    }
}

public struct LoginState: Codable {
    public let error: String
    public let onlineIp: String?
    public let userName: String?
    public let errorMsg: String?
    public let res: String?

    private enum CodingKeys: String, CodingKey {
        case error
        case onlineIp = "online_ip"
        case userName = "user_name"
        case errorMsg = "error_msg"
        case res
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.error = Self.decodeLossyString(container, key: .error) ?? ""
        self.onlineIp = Self.decodeLossyString(container, key: .onlineIp)
        self.userName = Self.decodeLossyString(container, key: .userName)
        self.errorMsg = Self.decodeLossyString(container, key: .errorMsg)
        self.res = Self.decodeLossyString(container, key: .res)
    }

    private static func decodeLossyString<K: CodingKey>(_ container: KeyedDecodingContainer<K>, key: K) -> String? {
        if let value = try? container.decodeIfPresent(String.self, forKey: key) {
            return value
        }
        if let value = try? container.decodeIfPresent(Int.self, forKey: key) {
            return String(value)
        }
        if let value = try? container.decodeIfPresent(Double.self, forKey: key) {
            return String(value)
        }
        if let value = try? container.decodeIfPresent(Bool.self, forKey: key) {
            return String(value)
        }
        return nil
    }

    public init(error: String, onlineIp: String? = nil, userName: String? = nil, errorMsg: String? = nil, res: String? = nil) {
        self.error = error
        self.onlineIp = onlineIp
        self.userName = userName
        self.errorMsg = errorMsg
        self.res = res
    }
}

public struct PortalProbe: Equatable {
    public var portalUrl: String?
    public var acid: UInt32?
    public var userIp: String?

    public init(portalUrl: String? = nil, acid: UInt32? = nil, userIp: String? = nil) {
        self.portalUrl = portalUrl
        self.acid = acid
        self.userIp = userIp
    }
}

public struct PortalProbeTrace: Identifiable {
    public let id = UUID()
    public let target: String
    public let status: Int?
    public let location: String?
    public let error: String?
    public let portalUrl: String?
    public let acid: UInt32?
    public let userIp: String?
    public let confirmsInternet: Bool

    public init(target: String, status: Int? = nil, location: String? = nil, error: String? = nil, portalUrl: String? = nil, acid: UInt32? = nil, userIp: String? = nil, confirmsInternet: Bool = false) {
        self.target = target
        self.status = status
        self.location = location
        self.error = error
        self.portalUrl = portalUrl
        self.acid = acid
        self.userIp = userIp
        self.confirmsInternet = confirmsInternet
    }
}
