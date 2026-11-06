import SwiftUI

// Implements [UI-COMPONENTS] and [DESIGN-COMPONENTS]: selectable rows. In
// the sidebar they are the native source list; elsewhere each row is a card,
// a full-row button whose selection shows as a tint and an outline, never
// colour alone.

/// Consecutive rows that name one group, drawn under its heading.
struct RowGroup: Identifiable {
    let id: Int
    let title: String
    var rows: [RowEntry]

    static func of(_ entries: [RowEntry]) -> [RowGroup] {
        entries.reduce(into: []) { groups, row in
            if groups.last?.title == row.group { groups[groups.count - 1].rows.append(row) } else { groups.append(RowGroup(id: groups.count, title: row.group, rows: [row])) }
        }
    }
}

struct RowsView: View {
    let entries: [RowEntry]
    let selected: String
    let drawing: Drawing
    let choose: (String) -> Void
    @Environment(\.ospreyPlacement) private var placement

    var body: some View {
        switch placement {
        case .sidebar: SidebarList(groups: RowGroup.of(entries), selected: selected, drawing: drawing, choose: choose)
        case .pane: ScrollView { cards.padding(.vertical, 2) }
        default: cards
        }
    }

    private var cards: some View {
        CardList(groups: RowGroup.of(entries), selected: selected, drawing: drawing) { key in if key != selected { choose(key) } }
    }
}

struct SidebarList: View {
    let groups: [RowGroup]
    let selected: String
    let drawing: Drawing
    let choose: (String) -> Void

    var body: some View {
        List(selection: Binding.sending(selected.isEmpty ? nil : selected) { key in if let key, key != selected { choose(key) } }) {
            ForEach(groups) { group in
                if group.title.isEmpty { rows(group) } else { Section(group.title) { rows(group) } }
            }
        }
        .listStyle(.sidebar)
    }

    private func rows(_ group: RowGroup) -> some View {
        ForEach(group.rows, id: \.key) { row in
            NodeView(node: row.child, drawing: drawing).environment(\.ospreyPlacement, .row).padding(.vertical, 5).tag(row.key)
        }
    }
}

struct CardList: View {
    let groups: [RowGroup]
    let selected: String
    let drawing: Drawing
    let choose: (String) -> Void
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.ospreyGap) private var gap

    var body: some View {
        LazyVStack(alignment: .leading, spacing: gap ?? theme.spacing(2)) {
            ForEach(groups) { group in
                if !group.title.isEmpty { TextView(value: group.title, role: .eyebrow, tone: .secondary).padding(.top, theme.spacing(1)) }
                ForEach(group.rows, id: \.key) { row in CardView(row: row, selected: row.key == selected, drawing: drawing) { choose(row.key) } }
            }
        }
    }
}

/// [DESIGN-COMPONENTS] finding row: symbol, content and a disclosure chevron on a 14-point card.
struct CardView: View {
    let row: RowEntry
    let selected: Bool
    let drawing: Drawing
    let tap: () -> Void
    @Environment(\.ospreyTheme) private var theme
    @Environment(\.colorScheme) private var scheme

    @Environment(\.ospreyFlatRows) private var flatRows

    private var dark: Bool { scheme == .dark }

    var body: some View {
        Button(action: tap) {
            HStack(alignment: .top, spacing: 14) {
                if !row.symbol.isEmpty { Image(systemName: row.symbol).font(.title2).frame(width: 30).padding(.top, 3).accessibilityHidden(true) }
                NodeView(node: row.child, drawing: drawing).environment(\.ospreyPlacement, .row)
                Spacer(minLength: 0)
                if !flatRows { Image(systemName: "chevron.right").font(.caption).foregroundStyle(theme.tone(.secondary, dark: dark)) }
            }
            .padding(flatRows ? 0 : 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .surface(flatRows ? (selected ? theme.color("selection", dark: dark) : .clear) : theme.color(selected ? "inset" : "surface", dark: dark), border: selected ? theme.tone(.accent, dark: dark) : theme.color("line", dark: dark), radius: flatRows ? 3 : 14, width: flatRows ? 0 : selected ? 1.5 : 0.5)
            .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
