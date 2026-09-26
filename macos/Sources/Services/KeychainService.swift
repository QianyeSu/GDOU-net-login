import Foundation
import Security

public enum KeychainService {
    private static let serviceName = "cn.gdou.gdou-net-login"

    /// A non-secret health check used by the settings screen.  It deliberately
    /// reports only whether the current account's item can be read; the
    /// password bytes are never exposed to the view layer.
    public enum PasswordStatus: Equatable {
        case available
        case missing
        case inaccessible(OSStatus)
    }

    public static func savePassword(_ password: String, forAccount account: String) -> Bool {
        guard !account.isEmpty else { return false }
        guard let data = password.data(using: .utf8) else { return false }

        deletePassword(forAccount: account)

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: account,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlock
        ]

        let status = SecItemAdd(query as CFDictionary, nil)
        return status == errSecSuccess
    }

    public static func loadPassword(forAccount account: String) -> String? {
        guard !account.isEmpty else { return nil }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess, let data = item as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    public static func passwordStatus(forAccount account: String) -> PasswordStatus {
        guard !account.isEmpty else { return .missing }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            return (item as? Data)?.isEmpty == false ? .available : .missing
        case errSecItemNotFound:
            return .missing
        default:
            return .inaccessible(status)
        }
    }

    @discardableResult
    public static func deletePassword(forAccount account: String) -> Bool {
        guard !account.isEmpty else { return false }

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: serviceName,
            kSecAttrAccount as String: account
        ]

        let status = SecItemDelete(query as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound
    }
}
