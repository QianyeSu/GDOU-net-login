import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private weak var mainWindow: NSWindow?
    private var mainMinimumFrameSize: NSSize?
    private var statusItem: NSStatusItem?
    // GDOU is a menu-bar agent, like Stats: its process remains alive while
    // the main window is hidden and it never owns a Dock tile.  The first
    // SwiftUI WindowGroup window is ordered out after it registers, and is
    // shown only after the user chooses the tray item (or reopens the app).
    private var mainWindowWasRequested = false
    private var pendingMainWindowRequest = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Set this before SwiftUI creates the WindowGroup.  This avoids a
        // transient Dock icon during launch and keeps the reconnect manager
        // running as a background tray process.
        NSApp.setActivationPolicy(.accessory)
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

        if pendingMainWindowRequest || mainWindowWasRequested {
            pendingMainWindowRequest = false
            DispatchQueue.main.async { [weak self] in
                self?.showMainWindowFromStatusItem()
            }
        } else {
            // WindowGroup normally creates its first window visible.  A
            // menu-bar-only app must start with that window hidden; otherwise
            // the app briefly appears in the Dock before the user clicks the
            // tray icon.
            window.orderOut(nil)
        }
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
        mainWindowWasRequested = true
        guard let window = mainWindow else {
            pendingMainWindowRequest = true
            return
        }
        configureMainWindow(window)
        // Keep the agent policy even while the settings window is visible.
        // This is the same behavior as Stats/iStat: the app is represented by
        // its menu-bar item, never by a Dock tile.
        NSApp.setActivationPolicy(.accessory)
        if window.isMiniaturized {
            window.deminiaturize(nil)
        }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        mainWindowWasRequested = true
        guard mainWindow != nil else {
            pendingMainWindowRequest = true
            return true
        }
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
            // Keep the tray and reconnect manager running.  The app already
            // has the accessory policy, so closing never creates a Dock tile.
            return false
        }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return false // Stay running in background
    }
}
