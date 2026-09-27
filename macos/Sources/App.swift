import SwiftUI
import AppKit

@main
struct GDOU_Net_LoginApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView()
                .background(MainWindowReader { window in
                    appDelegate.registerMainWindow(window)
                })
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 388, height: 655)
        .commands {
            CommandGroup(replacing: .newItem) {} // Remove Cmd+N
        }
    }
}
