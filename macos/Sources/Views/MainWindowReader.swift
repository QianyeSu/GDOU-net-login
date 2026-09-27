import SwiftUI
import AppKit

/// Registers only the NSWindow hosting ContentView, never status/menu windows.
struct MainWindowReader: NSViewRepresentable {
    let onResolve: (NSWindow) -> Void

    func makeNSView(context: Context) -> MainWindowReaderView {
        let view = MainWindowReaderView()
        view.onResolve = onResolve
        return view
    }

    func updateNSView(_ nsView: MainWindowReaderView, context: Context) {
        nsView.onResolve = onResolve
        nsView.resolveWindow()
    }
}

final class MainWindowReaderView: NSView {
    var onResolve: ((NSWindow) -> Void)?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        resolveWindow()
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func resolveWindow() {
        guard let window else { return }
        // Let SwiftUI finish attaching/layout before applying window geometry.
        DispatchQueue.main.async { [weak self, weak window] in
            guard let self, let window, self.window === window else { return }
            self.onResolve?(window)
        }
    }
}
