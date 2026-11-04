import SwiftUI

// Shared Osprey view nodes become native controls; all bank behavior stays in Update.
struct NativeNode: View {
    let node: BankNode
    @ObservedObject var store: BankStore
    var form: BankNode? = nil
    var parent: BankNode? = nil
    var dark = false
    var fieldLabel = ""
    @Environment(\.bankViewportWidth) private var viewportWidth
    @Environment(\.verticalSizeClass) private var verticalSize

    var body: some View { content.accessibilityHidden(node.props["aria-hidden"] == "true") }

    // A phone on its side has no height for a page's title block, and the top bar already names the page.
    private var isCollapsedTitle: Bool { verticalSize == .compact && node.tag == "div" && parent?.has("page-heading") == true }

    private var content: AnyView {
        if node.props["hidden"] == "true" || node.has("hero-art") || node.has("sr-only") || isCollapsedTitle { return AnyView(EmptyView()) }
        if node.has("loading-page") { return AnyView(ProgressView("Loading bank data").frame(maxWidth: .infinity, minHeight: 220)) }
        if node.has("security-orbit") { return AnyView(BankMonogram().frame(maxWidth: .infinity).frame(height: 150)) }
        if node.has("brand-mark") {
            return AnyView(Text(node.label).font(BankTheme.type(20, weight: .heavy)).foregroundColor(.white)
                .frame(width: 38, height: 38).background(BankTheme.coral).clipShape(RoundedRectangle(cornerRadius: 12)))
        }
        switch node.tag {
        case "input", "textarea": return AnyView(BankTextField(node: node, store: store, label: fieldLabel).id(node.id))
        case "select": return AnyView(BankPicker(node: node, store: store, label: fieldLabel))
        case "button", "a": return AnyView(button)
        case "hr": return AnyView(Divider())
        default: return node.nodes.isEmpty ? AnyView(text) : container
        }
    }

    private var container: AnyView {
        if node.has("empty-state") {
            return AnyView(VStack(spacing: 12) { children }.multilineTextAlignment(.center)
                .frame(maxWidth: .infinity, minHeight: 250).padding(20))
        }
        if viewportWidth >= 620, !node.classes.isDisjoint(with: ["account-grid", "stat-grid", "mini-stat-grid"]) {
            let columns = Array(repeating: GridItem(.flexible(), spacing: 14), count: viewportWidth >= 1180 ? 3 : 2)
            return AnyView(LazyVGrid(columns: columns, alignment: .leading, spacing: 14) { children })
        }
        if viewportWidth >= 620, node.has("paired-fields") {
            return AnyView(HStack(alignment: .lastTextBaseline, spacing: 12) { children })
        }
        if node.has("hero-card") {
            return AnyView(ZStack(alignment: .trailing) {
                BankMonogram().opacity(0.22).offset(x: 34)
                VStack(alignment: .leading, spacing: 20) { children }
            }.modifier(BankSurface(node: node)))
        }
        if node.has("segmented") {
            return AnyView(HStack(spacing: 4) { children }.padding(4)
                .background(Color(hex: 0xf2f4f0)).clipShape(RoundedRectangle(cornerRadius: 14)))
        }
        if node.has("filter-row") || node.has("detail-actions") {
            return AnyView(ScrollView(.horizontal, showsIndicators: false) { HStack(spacing: 8) { children } })
        }
        if node.isRow {
            return AnyView(HStack(alignment: .center, spacing: 12) { children }.modifier(BankSurface(node: node)))
        }
        return AnyView(VStack(alignment: node.has("movement-meta") ? .trailing : .leading, spacing: node.spacing) {
            if let value = node.text, !value.isEmpty { Text(value) }
            children
        }.modifier(BankSurface(node: node)))
    }

    private var children: some View {
        ForEach(Array(node.nodes.enumerated()), id: \.offset) { _, child in
            NativeNode(node: child, store: store, form: node.tag == "form" ? node : form,
                       parent: node, dark: dark || node.isDark,
                       fieldLabel: node.has("field") ? node.nodes.first?.text ?? "" : fieldLabel)
        }
    }

    private var text: some View {
        Text(node.text ?? "")
            .font(BankTheme.font(node, parent: parent))
            .foregroundColor(BankTheme.foreground(node, dark: dark))
            .tracking(node.has("eyebrow") ? 1.5 : 0)
            .monospacedDigit()
            .strikethrough(node.has("movement-amount") && node.has("refused"))
            .fixedSize(horizontal: false, vertical: true)
            .padding(node.isIcon || node.has("status-pill") ? 10 : 0)
            .background(BankTheme.badgeBackground(node))
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .accessibilityAddTraits(["h1", "h2", "h3"].contains(node.tag) ? .isHeader : [])
    }

    private var button: some View {
        Button(action: activate) {
            if node.has("account-card") {
                VStack(alignment: .leading, spacing: 14) { children }
                    .modifier(BankSurface(node: node))
                    .overlay(RoundedRectangle(cornerRadius: 20).stroke(node.has("selected") ? BankTheme.green : .clear, lineWidth: 2))
            } else {
                buttonLabel
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(node.accessibleLabel)
        .accessibilityAddTraits(node.has("active") || node.has("selected") ? .isSelected : [])
        .accessibilityIdentifier(node.id.isEmpty && node.props["type"] == "submit" ? "\(form?.id ?? "form")-button" : node.id)
        .disabled(node.props["disabled"] == "true" || (node.props["type"] == "submit" && store.busy))
        .opacity(node.props["type"] == "submit" && store.busy ? 0.55 : 1)
    }

    private var buttonLabel: some View {
        HStack(spacing: 8) {
            ForEach(Array(node.nodes.enumerated()), id: \.offset) { _, child in
                if !(node.has("icon-button") || node.has("toast-close")) || !child.has("button-label") { Text(child.label) }
            }
            if let value = node.text { Text(value) }
            if node.has("nav-item") { Spacer(minLength: 0) }
        }
        .font(BankTheme.type(node.has("chip") ? 12 : 14, weight: .semibold))
        .frame(minWidth: 24, minHeight: 24)
        .padding(.horizontal, node.has("chip") ? 9 : 14).padding(.vertical, 11)
        .frame(maxWidth: node.has("nav-item") || node.has("submit-button") || parent?.has("segmented") == true ? .infinity : nil, alignment: .leading)
        .foregroundColor(buttonForeground)
        .background(buttonBackground)
        .clipShape(RoundedRectangle(cornerRadius: parent?.has("segmented") == true ? 10 : node.has("chip") ? 20 : 12))
    }

    private var buttonForeground: Color {
        if node.has("primary") { return .white }
        if node.has("text-button") { return BankTheme.green }
        if node.has("active") {
            if node.has("nav-item") { return .white }
            return parent?.has("segmented") == true ? BankTheme.ink : .white
        }
        return dark ? .white : BankTheme.ink
    }
    private var buttonBackground: Color {
        if node.has("primary") { return BankTheme.coral }
        if node.has("active") {
            if node.has("nav-item") { return BankTheme.mint.opacity(0.14) }
            return parent?.has("segmented") == true ? .white : BankTheme.ink
        }
        if node.has("secondary") || node.has("chip") || node.has("icon-button") { return dark ? .white.opacity(0.08) : .white }
        return .clear
    }
    private func activate() {
        if node.props["type"] == "submit", let form { store.submit(form) }
        else { store.send(["kind": node.props["event"] ?? "click", "id": node.id]) }
    }
}

private struct BankMonogram: View {
    var body: some View {
        ZStack {
            Circle().stroke(BankTheme.mint.opacity(0.18), lineWidth: 1).frame(width: 150, height: 150)
            Circle().stroke(BankTheme.mint.opacity(0.25), lineWidth: 1).frame(width: 106, height: 106)
            Text("T").font(BankTheme.type(34, weight: .heavy)).foregroundColor(.white)
                .frame(width: 66, height: 66).background(BankTheme.coral)
                .clipShape(RoundedRectangle(cornerRadius: 22)).rotationEffect(.degrees(8))
        }.accessibilityHidden(true)
    }
}

struct BankTextField: View {
    let node: BankNode
    @ObservedObject var store: BankStore
    let label: String
    @FocusState private var focused: Bool
    // The field owns its text; the store only records it. Identity follows the node id.
    @State private var text = ""

    var body: some View {
        TextField(node.props["placeholder"] ?? "", text: Binding(get: { text }, set: { text = $0; store.edit(node, $0) }))
            .font(BankTheme.type(16)).foregroundColor(BankTheme.ink)
            .padding(14).frame(minHeight: 48).background(Color.white)
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(focused ? BankTheme.green : BankTheme.line))
            .focused($focused)
            .keyboardType(node.props["inputMode"] == "decimal" ? .decimalPad : .default)
            .textInputAutocapitalization(node.props["type"] == "search" ? .never : .words)
            .autocorrectionDisabled(node.props["type"] == "search")
            .accessibilityLabel(label.isEmpty ? node.props["placeholder"] ?? node.id : label)
            .accessibilityIdentifier(node.id)
            .onChange(of: store.focusID) { value in focused = value == node.id }
            .onAppear { text = store.value(node); focused = store.focusID == node.id }
    }
}

struct BankPicker: View {
    let node: BankNode
    @ObservedObject var store: BankStore
    let label: String
    private var selected: String {
        node.nodes.first { $0.props["value"] == store.value(node) }?.label ?? "Choose account"
    }
    var body: some View {
        Menu {
            ForEach(Array(node.nodes.enumerated()), id: \.offset) { _, option in
                Button(option.label) { store.choose(node, option.props["value"] ?? "") }
                    .accessibilityIdentifier("\(node.id)-option-\(option.props["value"] ?? "")")
            }
        } label: {
            HStack { Text(selected); Spacer(); Image(systemName: "chevron.down") }
                .font(BankTheme.type(14)).foregroundColor(BankTheme.ink)
                .padding(14).frame(minHeight: 48).background(Color.white)
                .overlay(RoundedRectangle(cornerRadius: 12).stroke(BankTheme.line))
        }
        .accessibilityLabel(label)
        .accessibilityValue(selected)
        .accessibilityIdentifier(node.id)
    }
}
