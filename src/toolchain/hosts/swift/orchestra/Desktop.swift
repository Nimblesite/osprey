import AppKit
import CoreImage

// [ORCHESTRA-PLAN-PLAYABLE] Native session controls; phones draw their expressive surface on canvas.
final class OrchestraDesktop: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private let status = NSTextField(labelWithString: "Starting audio and controller connections…")
    private let meter = NSLevelIndicator()
    private var invite = ""

    func show(invite: String) {
        self.invite = invite
        NSApplication.shared.setActivationPolicy(.regular)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 660, height: 560),
                              styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Light Orchestra"
        window.delegate = self
        window.isReleasedWhenClosed = false
        self.window = window
        installContent(in: window)
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    private func installContent(in window: NSWindow) {
        let content = NSStackView(views: headerViews() + inviteViews() + audioViews())
        content.orientation = .vertical
        content.alignment = .leading
        content.spacing = 12
        content.translatesAutoresizingMaskIntoConstraints = false
        guard let view = window.contentView else { return }
        view.addSubview(content)
        NSLayoutConstraint.activate([content.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 28),
                                     content.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -28),
                                     content.topAnchor.constraint(equalTo: view.topAnchor, constant: 24),
                                     meter.widthAnchor.constraint(equalTo: content.widthAnchor)])
    }

    private func headerViews() -> [NSView] {
        let title = NSTextField(labelWithString: "Your phones. One orchestra.")
        title.font = .systemFont(ofSize: 28, weight: .semibold)
        let hint = NSTextField(wrappingLabelWithString: "Join the same Wi-Fi, scan the code, then touch the ribbon. Sound plays here on your Mac.")
        return [title, hint]
    }

    private func inviteViews() -> [NSView] {
        let link = NSTextField(wrappingLabelWithString: invite)
        link.isSelectable = true
        link.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        let qr = NSImageView()
        qr.image = qrImage(invite)
        qr.setAccessibilityLabel("Scan this QR code to join the orchestra")
        qr.widthAnchor.constraint(equalToConstant: 180).isActive = true
        qr.heightAnchor.constraint(equalToConstant: 180).isActive = true
        let copy = NSButton(title: "Copy phone invite", target: self, action: #selector(copyInvite))
        let silence = NSButton(title: "Silence all notes", target: self, action: #selector(silenceAll))
        let controls = NSStackView(views: [copy, silence])
        controls.spacing = 12
        return [qr, link, controls]
    }

    private func audioViews() -> [NSView] {
        let volume = NSSlider(value: 0.65, minValue: 0, maxValue: 1, target: self, action: #selector(volumeChanged(_:)))
        volume.setAccessibilityLabel("Master volume")
        volume.widthAnchor.constraint(equalToConstant: 604).isActive = true
        meter.minValue = 0
        meter.maxValue = 1
        meter.levelIndicatorStyle = .continuousCapacity
        meter.setAccessibilityLabel("Output level")
        return [status, NSTextField(labelWithString: "Master volume"), volume, meter]
    }

    func update(peers: Int, notes: Int, level: Double) {
        status.stringValue = "Controllers: \(peers) · Notes: \(notes)"
        meter.doubleValue = level
    }

    @objc private func copyInvite() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(invite, forType: .string)
    }

    @objc private func silenceAll() {
        performSession {
            do { try server?.silence() } catch { failure = error.localizedDescription; requestStop() }
        }
    }

    @objc private func volumeChanged(_ sender: NSSlider) {
        let volume = Float(sender.doubleValue)
        performSession { audio.setVolume(volume) }
    }
    func windowWillClose(_ notification: Notification) { requestStop() }

    private func qrImage(_ text: String) -> NSImage? {
        guard let filter = CIFilter(name: "CIQRCodeGenerator") else { return nil }
        filter.setValue(Data(text.utf8), forKey: "inputMessage")
        guard let code = filter.outputImage else { return nil }
        let background = CIImage(color: .white).cropped(to: code.extent.insetBy(dx: -4, dy: -4))
        let image = code.composited(over: background).transformed(by: CGAffineTransform(scaleX: 8, y: 8))
        guard let bitmap = CIContext().createCGImage(image, from: image.extent) else { return nil }
        return NSImage(cgImage: bitmap, size: NSSize(width: 180, height: 180))
    }
}
