import SwiftUI

@main
struct TalonBankApp: App {
    @StateObject private var store = BankStore()
    var body: some Scene {
        WindowGroup {
            NavigationStack { BankScreen(store: store).toolbar(.hidden, for: .navigationBar) }
                .preferredColorScheme(.light)
                .task { store.start() }
        }
    }
}

struct BankScreen: View {
    @ObservedObject var store: BankStore
    @State private var settings = false
    private var sidebar: BankNode? { store.view?.first { $0.has("sidebar") } }
    private var menuOpen: Bool { sidebar?.has("open") == true }
    private var modal: BankNode? { store.view?.first { $0.has("modal") } }

    var body: some View {
        GeometryReader { geometry in
            let wide = geometry.size.width >= 860
            ZStack(alignment: .leading) {
                BankTheme.paper.ignoresSafeArea()
                HStack(spacing: 0) {
                    if wide { navigation.frame(width: 238) }
                    main(wide: wide)
                }
                .accessibilityHidden(modal != nil || (menuOpen && !wide))
                if menuOpen && !wide { drawer(width: min(geometry.size.width * 0.84, 300)) }
                if let modal { modalOverlay(modal) }
            }
            .overlay(alignment: .bottom) { if modal == nil { notice } }
            .environment(\.bankViewportWidth, geometry.size.width)
        }
        .tint(BankTheme.green)
        .font(BankTheme.type(14))
        .sheet(isPresented: $settings) { BankConnection(store: store) }
        .toolbar { ToolbarItemGroup(placement: .keyboard) {
            Spacer()
            Button("Done") { UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil) }
                .accessibilityIdentifier("keyboard-done")
        } }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.willEnterForegroundNotification)) { _ in store.click("topbar-refresh") }
    }

    private func main(wide: Bool) -> some View {
        VStack(spacing: 0) {
            if let header = store.view?.first({ $0.has("topbar") }) { BankHeader(node: header, store: store, wide: wide) }
            if store.view?.first({ $0.has("top-progress") }) != nil { ProgressView().tint(BankTheme.coral) }
            if let error = store.hostError { errorView(error) }
            else if let content = store.view?.first({ $0.has("content") }) {
                ScrollView { NativeNode(node: content, store: store).padding(20).padding(.bottom, 30) }
                    .id(store.route)
                    .accessibilityIdentifier("bank-content")
                    .scrollDismissesKeyboard(.interactively)
            } else { ProgressView("Starting Talon").frame(maxWidth: .infinity, maxHeight: .infinity) }
        }
    }

    private var navigation: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if let sidebar { NativeNode(node: sidebar, store: store, dark: true) }
                Button { settings = true } label: { Label("Server connection", systemImage: "network") }
                    .font(BankTheme.type(14)).foregroundColor(BankTheme.mint)
                    .padding(20).accessibilityIdentifier("server-settings")
            }
        }
        .frame(maxHeight: .infinity).background(BankTheme.ink.ignoresSafeArea())
        .accessibilityIdentifier("bank-navigation")
    }

    private func drawer(width: CGFloat) -> some View {
        // The scrim sits beside the drawer, not under it: a close button spanning the whole
        // screen takes the hit tests of the navigation buttons it overlaps.
        HStack(spacing: 0) {
            navigation.frame(width: width).shadow(radius: 20).zIndex(1)
            Color.black.opacity(0.35).ignoresSafeArea()
                .onTapGesture { store.click("toggle-menu") }
                .accessibilityLabel("Close navigation").accessibilityAddTraits(.isButton)
        }
        .accessibilityAction(.escape) { store.click("toggle-menu") }
    }

    @ViewBuilder private var notice: some View {
        if let toast = store.view?.first({ $0.has("toast") }) {
            // A container element keeps the identifier on the notice itself. Without it SwiftUI
            // stamps "bank-notice" over every child, and the dismiss button loses "dismiss-notice".
            NativeNode(node: toast, store: store, dark: true)
                .frame(maxWidth: 430).padding(12).shadow(radius: 10)
                .accessibilityElement(children: .contain).accessibilityIdentifier("bank-notice")
        }
    }

    private func modalOverlay(_ node: BankNode) -> some View {
        ZStack(alignment: .bottom) {
            Color.black.opacity(0.45).ignoresSafeArea().onTapGesture { store.click("close-modal") }
            // The notice stacks on the sheet. Laid over the screen it covered the sheet's title
            // and close button once the keyboard pushed the sheet to the top.
            VStack(spacing: 0) {
                notice
                ScrollView { NativeNode(node: node, store: store) }
                    .frame(maxWidth: 540, maxHeight: 490)
                    .background(Color.white).clipShape(RoundedRectangle(cornerRadius: 26))
                    .accessibilityIdentifier("bank-modal")
            }
        }
        .accessibilityAddTraits(.isModal)
        .accessibilityAction(.escape) { store.click("close-modal") }
    }

    private func errorView(_ error: String) -> some View {
        VStack(spacing: 20) {
            Text("Talon could not start").font(.title2.bold())
            Text(error).foregroundColor(BankTheme.red)
            Button("Server connection") { settings = true }.accessibilityIdentifier("server-settings")
        }.padding(24).frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct BankHeader: View {
    let node: BankNode
    @ObservedObject var store: BankStore
    var wide = false
    var body: some View {
        HStack(spacing: 8) {
            if !wide, let menu = node.first({ $0.id == "toggle-menu" }) { compactButton(menu) }
            if let title = node.first({ $0.has("topbar-title") }) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(title.nodes.first?.text ?? "TALON BANK").font(BankTheme.type(8, weight: .semibold)).foregroundColor(BankTheme.muted).lineLimit(1)
                    Text(title.nodes.last?.text ?? "Overview").font(BankTheme.type(18, weight: .bold)).foregroundColor(BankTheme.ink)
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            if let actions = node.first({ $0.has("topbar-actions") }) {
                ForEach(Array(actions.nodes.enumerated()), id: \.offset) { _, action in compactButton(action) }
            }
        }.padding(.horizontal, 16).padding(.vertical, 12).background(BankTheme.paper)
    }
    private func compactButton(_ action: BankNode) -> some View {
        Button { store.click(action.id) } label: {
            Text(action.nodes.first?.label ?? action.label).font(BankTheme.type(23, weight: .medium))
                .frame(width: 38, height: 42)
                .foregroundColor(action.has("primary") ? .white : BankTheme.ink)
                .background(action.has("primary") ? BankTheme.coral : .white)
                .clipShape(RoundedRectangle(cornerRadius: 12))
        }.buttonStyle(.plain)
            .accessibilityLabel(action.accessibleLabel)
            .accessibilityIdentifier(action.id)
    }
}

struct BankConnection: View {
    @ObservedObject var store: BankStore
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @State private var error = ""
    var body: some View {
        NavigationStack {
            Form {
                Section("Bank server") {
                    TextField("http://192.168.1.20:18790", text: $address)
                        .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                        .accessibilityIdentifier("server-address")
                    Text("Use your computer’s network address on a physical phone. The simulator can use 127.0.0.1.")
                }
                if !error.isEmpty { Text(error).foregroundColor(BankTheme.red) }
                Button("Connect") { connect() }.accessibilityIdentifier("server-connect")
            }
            .navigationTitle("Server connection")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .onAppear { address = store.serverAddress }
        }
    }
    private func connect() {
        do { try store.connect(address); dismiss() }
        catch { self.error = error.localizedDescription }
    }
}
