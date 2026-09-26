import SwiftUI
import AppKit

public struct LiquidGlassCardModifier: ViewModifier {
    var cornerRadius: CGFloat
    @Environment(\.colorScheme) var colorScheme

    public func body(content: Content) -> some View {
        content
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(.ultraThinMaterial)
            )
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(
                        LinearGradient(
                            colors: [
                                Color.white.opacity(colorScheme == .dark ? 0.22 : 0.6),
                                Color.white.opacity(colorScheme == .dark ? 0.05 : 0.15)
                            ],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ),
                        lineWidth: 1
                    )
            )
            .shadow(
                color: Color.black.opacity(colorScheme == .dark ? 0.35 : 0.06),
                radius: 12,
                x: 0,
                y: 6
            )
    }
}

public struct LiquidGlassButtonModifier: ViewModifier {
    var isPrimary: Bool
    var isDanger: Bool
    @Environment(\.colorScheme) var colorScheme

    public func body(content: Content) -> some View {
        content
            .font(.system(size: 14, weight: .semibold))
            .foregroundColor(.white)
            .frame(maxWidth: .infinity)
            .frame(height: 38)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(buttonGradient)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(
                        Color.white.opacity(0.28),
                        lineWidth: 1
                    )
            )
            .shadow(color: shadowColor, radius: 8, x: 0, y: 4)
    }

    private var buttonGradient: LinearGradient {
        if isDanger {
            return LinearGradient(
                colors: [Color.red.opacity(0.85), Color.red.opacity(0.95)],
                startPoint: .top,
                endPoint: .bottom
            )
        } else if isPrimary {
            return LinearGradient(
                colors: [Color(red: 0.22, green: 0.52, blue: 0.98), Color(red: 0.12, green: 0.40, blue: 0.90)],
                startPoint: .top,
                endPoint: .bottom
            )
        } else {
            return LinearGradient(
                colors: [Color.gray.opacity(0.4), Color.gray.opacity(0.6)],
                startPoint: .top,
                endPoint: .bottom
            )
        }
    }

    private var shadowColor: Color {
        if isDanger {
            return Color.red.opacity(0.3)
        } else if isPrimary {
            return Color.blue.opacity(0.35)
        } else {
            return Color.black.opacity(0.1)
        }
    }
}

public extension View {
    func liquidGlassCard(cornerRadius: CGFloat = 16) -> some View {
        self.modifier(LiquidGlassCardModifier(cornerRadius: cornerRadius))
    }

    func liquidGlassButton(isPrimary: Bool = true, isDanger: Bool = false) -> some View {
        self.modifier(LiquidGlassButtonModifier(isPrimary: isPrimary, isDanger: isDanger))
    }
}

// Visual Effect View for pure macOS native vibrancy
public struct VisualEffectView: NSViewRepresentable {
    public let material: NSVisualEffectView.Material
    public let blendingMode: NSVisualEffectView.BlendingMode

    public init(material: NSVisualEffectView.Material = .hudWindow, blendingMode: NSVisualEffectView.BlendingMode = .behindWindow) {
        self.material = material
        self.blendingMode = blendingMode
    }

    public func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blendingMode
        view.state = .active
        return view
    }

    public func updateNSView(_ nsView: NSVisualEffectView, context: Context) {
        nsView.material = material
        nsView.blendingMode = blendingMode
    }
}
