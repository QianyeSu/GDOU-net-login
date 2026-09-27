import AppKit
import SwiftUI

// A standalone AppKit regression test. It never instantiates ContentView or
// AutoReconnectManager, so it neither reads credentials nor changes networking.
@main
struct WindowLifecycleTests {
    static func check(_ condition: @autoclosure () -> Bool, _ message: String) {
        guard condition() else {
            fputs("FAIL: \(message)\n", stderr)
            exit(1)
        }
    }

    static func settle() {
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
    }

    struct WindowSnapshot {
        let window: NSWindow
        let style: NSWindow.StyleMask
        let delegate: AnyObject?
        let minSize: NSSize
        let maxSize: NSSize

        init(_ window: NSWindow) {
            self.window = window
            style = window.styleMask
            delegate = window.delegate
            minSize = window.minSize
            maxSize = window.maxSize
        }

        func verifyUnchanged() {
            check(window.styleMask == style, "non-main window style must not change")
            check(window.delegate === delegate, "non-main window delegate must not change")
            check(window.minSize == minSize && window.maxSize == maxSize,
                  "non-main window size limits must not change")
        }
    }

    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let delegate = AppDelegate()
        app.delegate = delegate
        let launch = Notification(name: NSApplication.didFinishLaunchingNotification)
        delegate.applicationDidFinishLaunching(launch)
        settle()

        // Test the real AppKit backing windows, not a mock of a status item.
        let statusWindows = app.windows.filter { String(describing: type(of: $0)) == "NSStatusBarWindow" }
        check(!statusWindows.isEmpty, "AppKit must create a real status-bar window")
        let traySnapshots = statusWindows.map(WindowSnapshot.init)
        func verifyTray() {
            traySnapshots.forEach { $0.verifyUnchanged() }
            for window in statusWindows {
                check(!window.styleMask.contains(.titled), "tray must remain borderless")
                check(!window.styleMask.contains(.closable), "tray must not become closable")
                check(!window.styleMask.contains(.miniaturizable), "tray must not become miniaturizable")
                check(window.standardWindowButton(.closeButton) == nil, "tray must not own a close button")
                check(window.standardWindowButton(.miniaturizeButton) == nil,
                      "tray must not own a minimize button")
            }
        }
        verifyTray()

        let show = NSSelectorFromString("showMainWindowFromStatusItem")
        _ = delegate.perform(show)
        settle()
        check(app.activationPolicy() == .accessory, "no registered main window means no arbitrary fallback")
        verifyTray()
        print("PASS: tray startup and show-before-main keep the status window untouched")

        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 440, height: 480),
                            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        let panelSnapshot = WindowSnapshot(panel)
        let main = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 388, height: 655),
                            styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        // A different title proves that registration is based on ContentView's
        // actual hosting window rather than a title match or windows.first.
        main.title = "Window lifecycle regression test"
        main.isReleasedWhenClosed = false
        main.contentView = NSHostingView(rootView: Color.clear.background(MainWindowReader { window in
            delegate.registerMainWindow(window)
        }))
        settle()
        check(main.delegate === delegate, "the view reader must register its hosting window")
        check(main.styleMask.contains([.titled, .closable, .miniaturizable, .resizable]),
              "only the main window receives the desktop window controls")
        check(main.standardWindowButton(.closeButton) != nil, "main close button is preserved")
        verifyTray()
        panelSnapshot.verifyUnchanged()
        print("PASS: SwiftUI reader registers only its own window; tray/panels remain unchanged")

        for window in statusWindows + [panel] {
            let notification = Notification(name: NSWindow.didBecomeMainNotification, object: window)
            NotificationCenter.default.post(notification)
            delegate.windowDidBecomeMain(notification)
        }
        verifyTray()
        panelSnapshot.verifyUnchanged()
        print("PASS: non-main focus notifications cannot add traffic lights to the tray")

        main.setContentSize(NSSize(width: 510, height: 720))
        let resizedFrame = main.frame.size
        delegate.registerMainWindow(main)
        delegate.windowDidBecomeMain(Notification(name: NSWindow.didBecomeMainNotification, object: main))
        check(main.frame.size == resizedFrame, "re-register/focus must preserve the user's resized window")
        print("PASS: re-registering/focusing preserves window size")

        for _ in 0..<3 {
            _ = delegate.perform(show)
            settle()
            check(main.isVisible && app.activationPolicy() == .regular, "show restores the main window and Dock mode")
            main.performClose(nil)
            settle()
            check(!main.isVisible && app.activationPolicy() == .accessory, "close hides UI and Dock, keeping the app alive")
            verifyTray()
        }
        check(!delegate.applicationShouldTerminateAfterLastWindowClosed(app), "closing must not quit the reconnect process")
        print("PASS: three show/close cycles preserve the tray and switch Dock policy correctly")

        _ = delegate.perform(show)
        main.performClose(nil)
        _ = delegate.perform(show)
        settle()
        check(main.isVisible && app.activationPolicy() == .regular, "immediate reopen must win over pending Dock hiding")
        verifyTray()
        print("PASS: close/reopen in the same run-loop turn does not hide the Dock incorrectly")

        main.performClose(nil)
        settle()
        check(!delegate.applicationShouldHandleReopen(app, hasVisibleWindows: false), "handle reopen with the registered window")
        settle()
        check(main.isVisible && app.activationPolicy() == .regular, "Finder/Dock reopen restores the UI")
        verifyTray()
        print("PASS: app reopen restores the registered main window without touching the tray")

        var staleCallbacks = 0
        let detachedReader = MainWindowReaderView()
        detachedReader.onResolve = { _ in staleCallbacks += 1 }
        panel.contentView?.addSubview(detachedReader)
        detachedReader.removeFromSuperview()
        settle()
        check(staleCallbacks == 0, "detached reader must discard a queued registration")
        panelSnapshot.verifyUnchanged()
        main.orderOut(nil)
        print("PASS: detached SwiftUI reader ignores stale window references")
        print("All window lifecycle regression tests passed.")
    }
}
