import AppKit
import SwiftUI

private struct ModalFocusKey: EnvironmentKey { static let defaultValue: ModalFocusBoundary? = nil }

extension EnvironmentValues {
    var ospreyModalFocusBoundary: ModalFocusBoundary? {
        get { self[ModalFocusKey.self] }
        set { self[ModalFocusKey.self] = newValue }
    }
}

private final class WeakModalFocusBoundary {
    weak var boundary: ModalFocusBoundary?
    init(_ boundary: ModalFocusBoundary) { self.boundary = boundary }
}

private final class ModalFocusStack {
    var entries: [WeakModalFocusBoundary] = []
    weak var previousResponder: NSResponder?
    func prune() { entries.removeAll { $0.boundary == nil } }
}

@MainActor
final class ModalFocusBoundary {
    private static let stacks = NSMapTable<NSWindow, ModalFocusStack>.weakToStrongObjects()
    private weak var window: NSWindow?
    private weak var previousResponder: NSResponder?

    static func permits(_ candidate: ModalFocusBoundary?, in window: NSWindow) -> Bool {
        guard let current = active(in: window) else { return true }
        return current === candidate
    }

    func attach(to window: NSWindow) {
        guard self.window !== window else { return }
        if let previous = self.window { Self.remove(self, from: previous, restore: true) }
        let stack = Self.stack(for: window)
        stack.prune()
        previousResponder = Self.logicalResponder(in: window)
        if stack.entries.isEmpty { stack.previousResponder = previousResponder }
        stack.entries.append(WeakModalFocusBoundary(self))
        self.window = window
    }

    func detach() {
        guard let window else { return }
        Self.remove(self, from: window, restore: true)
        self.window = nil
        previousResponder = nil
    }

    private static func active(in window: NSWindow) -> ModalFocusBoundary? {
        let stack = stacks.object(forKey: window)
        stack?.prune()
        return stack?.entries.last?.boundary
    }

    private static func stack(for window: NSWindow) -> ModalFocusStack {
        if let stack = stacks.object(forKey: window) { return stack }
        let stack = ModalFocusStack()
        stacks.setObject(stack, forKey: window)
        return stack
    }

    private static func remove(_ boundary: ModalFocusBoundary, from window: NSWindow, restore: Bool) {
        guard let stack = stacks.object(forKey: window) else { return }
        let wasActive = active(in: window) === boundary
        stack.entries.removeAll { $0.boundary == nil || $0.boundary === boundary }
        let target = stack.entries.isEmpty ? stack.previousResponder : boundary.previousResponder
        if stack.entries.isEmpty { stacks.removeObject(forKey: window) }
        if restore && wasActive { self.restore(target, in: window) }
    }

    private static func logicalResponder(in window: NSWindow) -> NSResponder? {
        let responder = window.firstResponder
        if let editor = responder as? NSTextView, editor.isFieldEditor, let control = editor.delegate as? NSView { return control }
        return responder
    }

    private static func restore(_ responder: NSResponder?, in window: NSWindow) {
        guard let responder else { window.makeFirstResponder(nil); return }
        if let view = responder as? NSView {
            guard view.window === window else { return }
        } else if responder !== window { return }
        window.makeFirstResponder(responder)
    }
}
