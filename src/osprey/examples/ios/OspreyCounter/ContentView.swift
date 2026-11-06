import SwiftUI

// Implements [IOS-SWIFT-HOST]: every count transition calls compiled Osprey.
struct ContentView: View {
    @State private var count = osprey_reset()
    @State private var message = HostLog.message

    var body: some View {
        VStack(spacing: 28) {
            title
            counter
            controls
            Text(message).font(.callout).foregroundColor(.secondary)
                .accessibilityIdentifier("hostMessage")
            Text("SwiftUI interface · Osprey logic")
                .font(.footnote).foregroundColor(.secondary)
        }
        .padding(32)
    }

    private var title: some View {
        VStack(spacing: 12) {
            Image(systemName: "number.circle.fill").font(.system(size: 48))
                .foregroundColor(.teal)
            Text(ospreyString(osprey_greeting()))
                .font(.largeTitle.bold()).multilineTextAlignment(.center)
            Text("Tap to run native Osprey code.")
                .font(.subheadline).foregroundColor(.secondary)
        }
    }

    private var counter: some View {
        VStack(spacing: 12) {
            Text("\(count)").font(.system(size: 80, weight: .semibold, design: .rounded))
                .monospacedDigit().accessibilityIdentifier("count")
            Text(osprey_milestone(count) ? "Five or more!" : "Try counting to five")
                .font(.headline).foregroundColor(.teal)
        }
        .frame(maxWidth: .infinity).padding(24)
        .background(Color.teal.opacity(0.08), in: RoundedRectangle(cornerRadius: 24))
    }

    private var controls: some View {
        HStack(spacing: 20) {
            Button("−") { update(osprey_decrement(count)) }
                .accessibilityLabel("Decrease count")
            Button("Reset") { update(osprey_reset()) }
            Button("+") { update(osprey_increment(count)) }
                .accessibilityLabel("Increase count")
        }
        .buttonStyle(.borderedProminent).tint(.teal).font(.title2)
    }

    private func update(_ value: Int64) {
        count = value
        let status = osprey_notify(count)
        message = status >= 0 ? HostLog.message : "Host callback failed: \(status)"
    }
}
