import AppKit
import SwiftUI

// Implements [UI-COMPONENTS-FRAMED]: authored paragraphs use word wrapping without orphan-word redistribution.
struct ParagraphText: NSViewRepresentable {
    let value: String
    let typography: FrameTypography
    let color: NSColor

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: value)
        field.isSelectable = true
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byWordWrapping
        paragraph.lineBreakStrategy = []
        paragraph.minimumLineHeight = typography.lineHeight
        paragraph.maximumLineHeight = typography.lineHeight
        let font = typography.nativeFont
        let baseline = font.leading + (typography.lineHeight - font.ascender + font.descender - font.leading) / 2
        let text = NSAttributedString(string: value, attributes: [.font: font, .foregroundColor: color, .paragraphStyle: paragraph, .kern: typography.tracking, .baselineOffset: baseline])
        if !field.attributedStringValue.isEqual(to: text) { field.attributedStringValue = text }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextField, context: Context) -> CGSize? {
        guard let width = proposal.width, width > 0 else { return nil }
        nsView.preferredMaxLayoutWidth = width
        return CGSize(width: width, height: nsView.intrinsicContentSize.height)
    }
}
