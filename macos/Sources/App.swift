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

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private weak var mainWindow: NSWindow?
    private var mainMinimumFrameSize: NSSize?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // WindowGroup can create its NSWindow on the next run-loop turn.  The
        // old implementation configured `windows.first` too early, which made
        // resizing/zooming and the title-bar buttons depend on launch timing.
        DispatchQueue.main.async { [weak self] in
            self?.configureWindows()
        }
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleWindowDidBecomeMain(_:)),
            name: NSWindow.didBecomeMainNotification,
            object: nil
        )
    }

    @objc private func handleWindowDidBecomeMain(_ notification: Notification) {
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
        window.delegate = self
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.styleMask.insert([.fullSizeContentView, .titled, .closable, .miniaturizable])
        // This is a compact desktop utility, not a document editor.  Do not
        // expose macOS Spaces full-screen for this window: SwiftUI's compact
        // layout is intentionally bounded and entering a separate full-screen
        // Space used to leave a black background around the small content.
        // Keep the compact Rust-client minimum, but allow normal edge
        // resizing.  The old macOS port removed `.resizable`, so dragging the
        // window only exposed desktop around a 388x655 card instead of letting
        // the SwiftUI layout grow with the window.  Full-screen remains
        // disabled separately below.
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
            window.styleMask.insert(.resizable)
            // Only restore the compact default on the first configuration.
            // didBecomeMain fires again whenever the user focuses the window;
            // resetting the content size there would undo every resize.
            if mainMinimumFrameSize == nil {
                window.setContentSize(compactSize)
                // NSWindow's min/max values are frame sizes (including the
                // title bar), so retain the calculated compact frame size.
                mainMinimumFrameSize = window.frame.size
            }
            window.minSize = mainMinimumFrameSize ?? window.frame.size
            window.maxSize = NSSize(width: 760, height: 1000)
        } else {
            // Settings/diagnostic sheets use their own SwiftUI frame.
            window.minSize = NSSize(width: 500, height: 430)
            window.maxSize = NSSize(width: 620, height: 600)
        }
        if !window.isVisible {
            window.center()
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        // Rust/Tauri hides the main window and keeps its watcher/tray alive
        // when the close traffic light is clicked.  Doing the same here keeps
        // AutoReconnectManager alive instead of destroying the SwiftUI view
        // and silently stopping reconnects.
        if sender === mainWindow {
            sender.orderOut(nil)
            return false
        }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false // Stay running in background
    }
}
