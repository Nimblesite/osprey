import SwiftUI

#if os(macOS)
import AppKit
#endif

// Implements [NATIVE-SHM-VIEW]'s editing half: a multiline field whose every edit delivers the
// whole text, and the picture a person pastes into it, saved as a PNG and named in the words by
// a marker the application reads.

/// A field of many lines, held on one row index so the caret and the keyboard focus survive the
/// view being re-read after each edit.
struct SharedEditor: View {
    @Binding var text: String
    let look: SharedLook
    let index: Int
    let control: RowControl
    @FocusState.Binding var focus: Int?
    @State private var pasteMonitor: Any?
    @State private var pasteID = UUID()

    var body: some View {
        TextEditor(text: $text)
            .font(look.font)
            .foregroundStyle(look.ink ?? .primary)
            .scrollContentBackground(.hidden)
            .frame(minHeight: look.height > 0 ? look.height : 60)
            .focused($focus, equals: index)
            .id(index)
            .disabled(look.disabled)
        #if os(macOS)
            .onAppear { if pasteMonitor == nil { pasteMonitor = monitorPaste() } }
            .onChange(of: focus == index) { _, now in
                if now { PastedImage.receivers.insert(pasteID) } else { PastedImage.receivers.remove(pasteID) }
            }
            .onDisappear {
                PastedImage.receivers.remove(pasteID)
                if let pasteMonitor { NSEvent.removeMonitor(pasteMonitor) }
                self.pasteMonitor = nil
            }
        #endif
    }

    #if os(macOS)
    /// Cmd-V with a picture on the clipboard: a text field takes only text, so the picture is
    /// saved and its marker joins the words. Anything else is left to the field.
    private func monitorPaste() -> Any? {
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard focus == index, event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command,
                  event.charactersIgnoringModifiers == "v", let marker = PastedImage.take() else { return event }
            text = text.isEmpty ? marker : "\(text) \(marker)"
            return nil
        }
    }
    #endif
}

/// The picture a row names, by URL or by path; no source draws nothing.
struct SharedPicture: View {
    let source: String

    var body: some View {
        if let url = SharedPicture.url(source) {
            AsyncImage(url: url) { image in
                image.resizable().aspectRatio(contentMode: .fit)
            } placeholder: {
                Color.clear
            }
        }
    }

    private static func url(_ words: String) -> URL? {
        guard !words.isEmpty else { return nil }
        if let url = URL(string: words), url.scheme != nil { return url }
        return URL(fileURLWithPath: words)
    }
}
