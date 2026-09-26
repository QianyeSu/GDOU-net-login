import SwiftUI
import AppKit

/// A small, non-interactive region that participates in native window drags.
///
/// `isMovableByWindowBackground` is not enough for a hidden-title-bar SwiftUI
/// window: a ScrollView/NSClipView becomes the hit-test target over most of
/// the card, so only the 24 px title-bar strip can be dragged.  Embedding this
/// view as the background of the non-control cards gives the utility several
/// safe drag handles while leaving text fields, buttons and the waveform's
/// hover area in charge of their own mouse events.
struct WindowDragView: NSViewRepresentable {
    func makeNSView(context: Context) -> DragNSView {
        DragNSView()
    }

    func updateNSView(_ nsView: DragNSView, context: Context) {}
}

final class DragNSView: NSView {
    override var mouseDownCanMoveWindow: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        wantsLayer = false
    }
}

extension View {
    /// Add a native drag target without changing layout or visual appearance.
    func windowDragArea() -> some View {
        background(WindowDragView().allowsHitTesting(true))
    }
}
