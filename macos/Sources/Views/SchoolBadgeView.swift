import SwiftUI
import AppKit

public struct SchoolBadgeView: View {
    @Environment(\.colorScheme) var colorScheme

    public var body: some View {
        ZStack {
            if let image = loadBadgeImage() {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 38, height: 38)
                    .shadow(color: Color(red: 0.0, green: 0.23, blue: 0.56).opacity(0.18), radius: 2, x: 0, y: 1)
            } else {
                // Fallback elegant vector icon
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .font(.system(size: 24, weight: .bold))
                    .foregroundColor(Color(red: 0.0, green: 0.23, blue: 0.56))
            }
        }
        .frame(width: 54, height: 54)
        .background(
            RoundedRectangle(cornerRadius: 15, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: colorScheme == .dark ?
                            [Color(red: 0.12, green: 0.16, blue: 0.23), Color(red: 0.06, green: 0.09, blue: 0.16)] :
                            [Color.white, Color(red: 0.97, green: 0.98, blue: 0.99)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
        )
        .overlay(
            RoundedRectangle(cornerRadius: 15, style: .continuous)
                .stroke(
                    LinearGradient(
                        colors: [
                            Color.white.opacity(colorScheme == .dark ? 0.2 : 0.9),
                            Color.primary.opacity(colorScheme == .dark ? 0.1 : 0.08)
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    lineWidth: 1
                )
        )
        .shadow(
            color: Color(red: 0.0, green: 0.23, blue: 0.56).opacity(colorScheme == .dark ? 0.35 : 0.08),
            radius: 8,
            x: 0,
            y: 4
        )
    }

    private func loadBadgeImage() -> NSImage? {
        if let path = Bundle.main.path(forResource: "icon", ofType: "png"),
           let img = NSImage(contentsOfFile: path) {
            return img
        }
        if let resourceUrl = Bundle.main.resourceURL?.appendingPathComponent("icon.png"),
           let img = NSImage(contentsOf: resourceUrl) {
            return img
        }
        // Also check direct path for preview / dev
        let devPath = "/Volumes/TiPlus7100s/GitHub/GDOU-net-login/macos/Resources/icon.png"
        if let img = NSImage(contentsOfFile: devPath) {
            return img
        }
        return NSImage(named: "AppIcon")
    }
}
