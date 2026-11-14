import Foundation
import Darwin
import AppKit

// [ORCHESTRA-EFFECTS] All Osprey and audio production run on one dedicated OS thread.
let environment = ProcessInfo.processInfo.environment
let audio = OrchestraAudio()
let desktop = OrchestraDesktop()
var server: OrchestraServer?
var timer: Timer?
var failure: String?
var lastSessionTick = -1.0
let began = ProcessInfo.processInfo.systemUptime
let headless = environment["ORCHESTRA_HEADLESS"] == "1"
let showWindow = !headless && environment["ORCHESTRA_NO_WINDOW"] != "1"
let duration = Double(environment["ORCHESTRA_DURATION"] ?? "0") ?? 0
let port = UInt16(environment["ORCHESTRA_PORT"] ?? "9090") ?? 9090
var host = environment["ORCHESTRA_HOST"] ?? "127.0.0.1"
let token = environment["ORCHESTRA_TOKEN"] ?? UUID().uuidString
let assets = environment["ORCHESTRA_CONTROLLER"] ?? Bundle.main.path(forResource: "controller", ofType: "html") ?? "src/osprey/examples/pocket_orchestra/controller.html"

// A fixed thread preserves the Osprey runtime's thread-local allocation/collector state.
final class SessionWork: NSObject {
    let run: () -> Void
    init(_ run: @escaping () -> Void) { self.run = run }
}
final class SessionThread: NSObject {
    lazy var thread = Thread(target: self, selector: #selector(startLoop), object: nil)
    @objc private func startLoop() { runSession() }
    @objc private func execute(_ work: SessionWork) { work.run() }
    func submit(_ work: @escaping () -> Void) {
        perform(#selector(execute(_:)), on: thread, with: SessionWork(work), waitUntilDone: false,
                modes: [RunLoop.Mode.common.rawValue])
    }
}
let session = SessionThread()
func performSession(_ work: @escaping () -> Void) { session.submit(work) }
func requestStop() { performSession { CFRunLoopStop(CFRunLoopGetCurrent()) } }
func stopApplication() {
    NSApplication.shared.stop(nil)
    if let event = NSEvent.otherEvent(with: .applicationDefined, location: .zero, modifierFlags: [], timestamp: 0,
                                     windowNumber: 0, context: nil, subtype: 0, data1: 0, data2: 0) {
        NSApplication.shared.postEvent(event, atStart: true)
    }
}

let diagnostics = OrchestraDiagnostics()
func log(_ event: String, _ fields: [String: Any] = [:]) { diagnostics.record(event, fields) }
var lastHealth = -1.0

func advance() {
    let elapsed = ProcessInfo.processInfo.systemUptime - began
    if elapsed - lastSessionTick >= 0.05 {
        if osprey_orchestra_tick(Int64(elapsed * 1000)) != 0 { failure = "Session clock rejected" }
        do { try audio.update() } catch { failure = error.localizedDescription }
        server?.expire()
        let peers = server?.controllerCount ?? 0, notes = audio.activeNotes, level = audio.level
        if showWindow { DispatchQueue.main.async { desktop.update(peers: peers, notes: notes, level: level) } }
        lastSessionTick = elapsed
    }
    if headless { _ = audio.render(128) } else { audio.fill() }
    if elapsed - lastHealth >= 1 {
        if !headless { audio.recoverOutput() }
        log("audio.health", ["frames": audio.renderedFrames, "notes": audio.activeNotes,
                             "controllers": server?.controllerCount ?? 0, "running": audio.isRunning,
                             "queued": orchestra_ring_queued(), "underruns": orchestra_ring_underruns(), "level": audio.level,
                             "maxCallbackFrames": orchestra_ring_max_read(), "maxRefillGapMs": audio.maxRefillGap * 1000])
        lastHealth = elapsed
    }
    failure = failure ?? server?.failure ?? audio.failure
    if failure != nil || (duration > 0 && elapsed >= duration) { requestStop() }
}

func start() throws {
    try validateConfiguration(environment)
    if !showWindow && environment["ORCHESTRA_HOST"] == nil { host = try localAddress() }
    guard osprey_main() == 0 else { throw problem("Osprey kernel initialization failed") }
    guard token.count >= 20 else { throw problem("Session token must contain at least 20 characters") }
    audio.capturesAudio = environment["ORCHESTRA_CAPTURE"] != nil
    let page = try Data(contentsOf: URL(fileURLWithPath: assets))
    let hostServer = OrchestraServer(audio: audio, host: host, token: token, page: page, port: port)
    try hostServer.start(port: port)
    server = hostServer
    if !headless { try audio.start(); audio.setVolume(0.65) }
    log("session.starting", ["address": "http://\(host):\(port)/", "headless": headless,
                            "sampleRate": audio.sampleRate, "transport": "trusted-LAN HTTP/WebSocket"])
    let clock = Timer(timeInterval: OrchestraAudioDeadline.interval, repeats: true) { _ in advance() }
    RunLoop.current.add(clock, forMode: .common)
    timer = clock
}

signal(SIGINT, SIG_IGN)
signal(SIGTERM, SIG_IGN)
let interrupt = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
let terminate = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
interrupt.setEventHandler { requestStop() }
terminate.setEventHandler { requestStop() }
interrupt.resume()
terminate.resume()
func runSession() {
    let deadline = OrchestraAudioDeadline()
    do {
        try start()
        if !headless { try deadline.start() }
        CFRunLoopRun()
    } catch { failure = error.localizedDescription }
    do { try deadline.stop() } catch { failure = failure ?? error.localizedDescription }
    timer?.invalidate()
    server?.stop()
    audio.stop()
    if let path = environment["ORCHESTRA_CAPTURE"] {
        do { try audio.writeCapture(to: path) } catch { failure = error.localizedDescription }
    }
    log("session.stopped", ["frames": audio.renderedFrames, "peak": audio.peak,
                            "underruns": orchestra_ring_underruns(), "error": failure ?? "",
                            "maxCallbackFrames": orchestra_ring_max_read(), "maxRefillGapMs": audio.maxRefillGap * 1000])
    diagnostics.flush()
    DispatchQueue.main.async { stopApplication() }
}
do { try diagnostics.start() } catch { log("logging.failed", ["error": error.localizedDescription]) }
// Build AppKit controls before audio starts; UI work cannot delay the producer thread.
if showWindow {
    do {
        if environment["ORCHESTRA_HOST"] == nil { host = try localAddress() }
        desktop.show(invite: "http://\(host):\(port)/#\(token)")
    } catch { failure = error.localizedDescription }
}
if failure == nil {
    session.thread.qualityOfService = .userInteractive
    session.thread.start()
    NSApplication.shared.run()
}
if let failure {
    FileHandle.standardError.write(Data("\(failure)\n".utf8))
    if showWindow {
        let alert = NSAlert()
        alert.messageText = "Orchestra session stopped"
        alert.informativeText = failure
        alert.runModal()
    }
    exit(1)
}
