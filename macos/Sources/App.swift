import SwiftUI
import AppKit

@main
struct GDOU_Net_LoginApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 388, height: 655)
        .commands {
            CommandGroup(replacing: .newItem) {} // Remove Cmd+N
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private weak var mainWindow: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // WindowGroup can create its NSWindow on the next run-loop turn.  The
        // old implementation configured `windows.first` too early, which made
        // resizing/zooming and the title-bar buttons depend on launch timing.
        DispatchQueue.main.async { [weak self] in
            self?.configureWindows()
        }
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(windowDidBecomeMain(_:)),
            name: NSWindow.didBecomeMainNotification,
            object: nil
        )
    }

    @objc private func windowDidBecomeMain(_ notification: Notification) {
        if let window = notification.object as? NSWindow {
            if mainWindow == nil, window.title == "GDOU Net Login" {
                mainWindow = window
            }
            configure(window, isMainWindow: window === mainWindow)
        }
    }

    private func configureWindows() {
        let windows = NSApplication.shared.windows
        if mainWindow == nil {
            mainWindow = windows.first(where: { $0.title == "GDOU Net Login" }) ?? windows.first
        }
        windows.forEach { configure($0, isMainWindow: $0 === mainWindow) }
    }

    private func configure(_ window: NSWindow) {
        configure(window, isMainWindow: window === mainWindow)
    }

    private func configure(_ window: NSWindow, isMainWindow: Bool) {
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.styleMask.insert([.fullSizeContentView, .titled, .closable, .miniaturizable])
        // This is a compact desktop utility, not a document editor.  Do not
        // expose macOS Spaces full-screen for this window: SwiftUI's compact
        // layout is intentionally bounded and entering a separate full-screen
        // Space used to leave a black background around the small content.
        // Keep the main utility fixed-size.  Otherwise a transparent
        // NSWindow can be dragged much wider than its SwiftUI content, which
        // leaves the desktop visible on both sides of the white content.
        window.collectionBehavior.insert(.fullScreenNone)
        window.collectionBehavior.remove(.fullScreenPrimary)
        window.collectionBehavior.remove(.fullScreenAuxiliary)
        window.styleMask.remove(.fullScreen)
        window.isMovableByWindowBackground = true
        window.isOpaque = false
        window.backgroundColor = .clear
        window.standardWindowButton(.zoomButton)?.isEnabled = false
        window.standardWindowButton(.zoomButton)?.isHidden = true

        if isMainWindow {
            let compactSize = NSSize(width: 388, height: 655)
            window.styleMask.remove(.resizable)
            window.minSize = compactSize
            window.maxSize = compactSize
            // Collapse frames saved by the old resizable build.
            if window.frame.size != compactSize {
                window.setContentSize(compactSize)
            }
        } else {
            // Settings/diagnostic sheets use their own SwiftUI frame.
            window.minSize = NSSize(width: 540, height: 620)
            window.maxSize = NSSize(width: 680, height: 780)
        }
        if !window.isVisible {
            window.center()
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false // Stay running in background
    }
}
