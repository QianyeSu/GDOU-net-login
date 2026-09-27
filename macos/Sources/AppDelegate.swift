import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private weak var mainWindow: NSWindow?
    private var mainMinimumFrameSize: NSSize?
    private var statusItem: NSStatusItem?

    func applicationDidFinishLaunching(_ notification: Notification) {
        installStatusItem()
        // ContentView registers its own window through MainWindowReader.
        // NSApp.windows also includes AppKit's NSStatusBarWindow; configuring
        // that list adds title-bar traffic lights to the menu-bar icon.
    }

    func registerMainWindow(_ window: NSWindow) {
        guard mainWindow !== window else { return }
        mainWindow = window
        mainMinimumFrameSize = nil
        configureMainWindow(window)
    }

    private func installStatusItem() {
        guard statusItem == nil else { return }
        let item = NSStatusBar.system.statusItem(withLength: 18)
        statusItem = item
        if let button = item.button {
            // Match the Rust/Tauri client: use the application's default
            // icon as the single menu-bar item. It is constrained to the
            // standard status-item slot and does not add text.
            // Load our bundled artwork without mutating the shared app icon.
            let iconPath = Bundle.main.path(forResource: "AppIcon", ofType: "icns")
            let image = iconPath.flatMap { NSImage(contentsOfFile: $0) }
                ?? NSImage(named: NSImage.applicationIconName)
                ?? NSImage(systemSymbolName: "wifi", accessibilityDescription: "GDOU 校园网")
            image?.size = NSSize(width: 18, height: 18)
            button.image = image
            button.title = ""
            button.imagePosition = .imageOnly
            button.imageScaling = .scaleProportionallyDown
            button.toolTip = "GDOU 校园网"
        }
        let menu = NSMenu()
        let show = NSMenuItem(title: "显示主窗口", action: #selector(showMainWindowFromStatusItem), keyEquivalent: "")
        show.target = self
        menu.addItem(show)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "退出 GDOU Net Login", action: #selector(terminateFromStatusItem), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
        item.menu = menu
    }

    @objc private func showMainWindowFromStatusItem() {
        // Never fall back to windows.first: it may be the tray's own window.
        guard let window = mainWindow else { return }
        configureMainWindow(window)
        // Return to a normal app before showing the window, so its Dock icon
        // and application menu are available only while the UI is open.
        NSApp.setActivationPolicy(.regular)
        if window.isMiniaturized {
            window.deminiaturize(nil)
        }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        guard mainWindow != nil else { return true }
        showMainWindowFromStatusItem()
        return false
    }

    @objc private func terminateFromStatusItem() {
        NSApp.terminate(nil)
    }

    func windowDidBecomeMain(_ notification: Notification) {
        guard let window = notification.object as? NSWindow,
              window === mainWindow else { return }
        configureMainWindow(window)
    }

    private func configureMainWindow(_ window: NSWindow) {
        guard window === mainWindow else { return }
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

        let compactSize = NSSize(width: 388, height: 655)
        window.styleMask.insert(.resizable)
        // Re-focusing a window must preserve the user's resized dimensions.
        if mainMinimumFrameSize == nil {
            window.setContentSize(compactSize)
            // NSWindow min/max values are frame sizes, including the title bar.
            mainMinimumFrameSize = window.frame.size
        }
        window.minSize = mainMinimumFrameSize ?? window.frame.size
        window.maxSize = NSSize(width: 760, height: 1000)
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
            // Keep the tray and reconnect manager running, but remove the
            // application's Dock icon after the main window is hidden.
            DispatchQueue.main.async { [weak self] in
                // A reopen queued in the same run-loop turn takes precedence.
                guard let window = self?.mainWindow, !window.isVisible else { return }
                NSApp.setActivationPolicy(.accessory)
            }
            return false
        }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false // Stay running in background
    }
}
