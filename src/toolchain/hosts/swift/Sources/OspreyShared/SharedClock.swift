import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

// Implements [UI-SCENE]'s frame half: the display link an `animate` ask turns on. While it
// runs, every display refresh reaches the application as a `frame` event, `elapsed` seconds
// after the last — the first 1/60th of a second, and never more than a tenth, so a window the
// system paused cannot deliver a jump the application did not ask for.

/// The host's frame clock, off until an `animate` ask turns it on.
final class SharedClock: @unchecked Sendable {
    /// One tick, on the main thread: the seconds since the last.
    var onTick: (@MainActor (Double) -> Void)?

    private var running = false
    private var last: Double = 0
    private var link: CADisplayLink?

    /// Turns the clock on or off; nothing changes while it already sits that way.
    func setRunning(_ on: Bool) {
        DispatchQueue.main.async { [self] in
            guard on != running else { return }
            on ? start() : stop()
        }
    }

    @MainActor
    private func start() {
        running = true
        last = 0
        #if os(macOS)
        link = NSScreen.main?.displayLink(target: ClockProxy(self), selector: #selector(ClockProxy.tick))
        #else
        link = CADisplayLink(target: ClockProxy(self), selector: #selector(ClockProxy.tick))
        #endif
        link?.add(to: .main, forMode: .common)
    }

    @MainActor
    private func stop() {
        running = false
        link?.invalidate()
        link = nil
    }

    /// One display refresh arrived; its elapsed time is measured on the main thread, where the
    /// tick is delivered, so a paused window's backlog cannot bunch up.
    @MainActor
    fileprivate func arrive() {
        guard running else { return }
        let at = CFAbsoluteTimeGetCurrent()
        let elapsed = last == 0 ? 1.0 / 60 : min(at - last, 0.1)
        last = at
        onTick?(elapsed)
    }

    /// A weak target, so the display link never keeps the clock alive.
    private final class ClockProxy: NSObject {
        weak var clock: SharedClock?
        init(_ clock: SharedClock) { self.clock = clock }
        @objc func tick() { Task { @MainActor in clock?.arrive() } }
    }
}
