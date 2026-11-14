// [ORCHESTRA-PLAN-ENSEMBLE] Real native kernel, HTTP/WebSockets, phone browser and captured PCM.
import assert from 'node:assert/strict'
import { spawn, execFileSync } from 'node:child_process'
import { readFile, mkdir, readdir, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
const require = createRequire(resolve('src/toolchain/website/package.json'))
const WebSocket = require('ws')
const { chromium, webkit } = require('@playwright/test')
const out = resolve('target/orchestra')
await mkdir(out, { recursive: true })
const windowed = process.env.ORCHESTRA_DEVICE_TEST === '1'
assert.equal((await readFile(`${out}/Light Orchestra.app/Contents/Resources/AppIcon.icns`)).toString('ascii', 0, 4), 'icns')
await verifySoundPalette()
if (windowed) await verifyDeviceRecovery()
const token = 'orchestra-integration-private-token'
const port = 19140
const origin = `http://127.0.0.1:${port}`
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
let output = '', errors = '', browser, child, exited
// The audio recording has its own short session. Browser journeys can then take
// as long as their interaction deadlines permit without filling that recording.
function startSession(capture) {
  output = ''; errors = ''
  const env = { ...process.env,
    ORCHESTRA_NO_WINDOW: windowed ? '0' : '1', ORCHESTRA_HEADLESS: windowed ? '0' : '1',
    ORCHESTRA_LOG_DIRECTORY: `${out}/diagnostics`, ORCHESTRA_PORT: String(port), ORCHESTRA_TOKEN: token, ORCHESTRA_DURATION: '0' }
  delete env.ORCHESTRA_CAPTURE
  if (capture) env.ORCHESTRA_CAPTURE = `${out}/controller-audio.f32`
  child = spawn(resolve('target/orchestra/pocket-orchestra'), [], { env })
  child.stdout.on('data', data => { output += data })
  child.stderr.on('data', data => { errors += data })
  exited = new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })))
}
startSession(true)
const clients = []
const silentIntervals = []
const toneIntervals = []
function deadline(promise, label) {
  let timer
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(`Timeout: ${label}`)), 5000) })]).finally(() => clearTimeout(timer))
}
// Raw fixtures disable automatic pongs to exercise explicit heartbeat expiry.
async function connect(allowedOrigin = origin, autoPong = false) {
  const ws = new WebSocket(`ws://127.0.0.1:${port + 1}`, 'orchestra.v1', { origin: allowedOrigin, autoPong })
  clients.push(ws)
  await deadline(new Promise((resolve, reject) => { ws.once('open', resolve); ws.once('error', reject) }), 'WebSocket open')
  return ws
}
async function request(ws, message) {
  const reply = deadline(new Promise((resolve, reject) => {
    const closed = () => reject(Error('Closed before response'))
    ws.once('close', closed)
    ws.once('message', data => { ws.off('close', closed); resolve(JSON.parse(data)) })
  }), message.type)
  ws.send(JSON.stringify(message))
  return reply
}
const touch = (sequence, phase, finger = 0, x = .5, y = .8) => ({ type: 'touch', sequence, phase, finger, x, y })
async function joined(autoPong = false) {
  const ws = await connect(origin, autoPong)
  assert.equal((await request(ws, { type: 'join', token })).type, 'joined')
  return ws
}
async function observedNotes() {
  // Each probe lives for one request, so browser work cannot expire its lease.
  const observer = await joined()
  try { return (await request(observer, { type: 'heartbeat' })).notes }
  finally { observer.close() }
}
async function waitForSession() {
  for (let attempt = 0; ; attempt++) {
    try { assert.equal((await fetch(origin)).status, 200); break }
    catch (error) { if (attempt === 60) throw error; await delay(50) }
  }
}
try {
  await waitForSession()
  const collision = spawn(resolve('target/orchestra/pocket-orchestra'), [], { env: { ...process.env,
    ORCHESTRA_HEADLESS: '1', ORCHESTRA_NO_WINDOW: '1', ORCHESTRA_PORT: String(port),
    ORCHESTRA_TOKEN: token, ORCHESTRA_DURATION: '2' } })
  let collisionOutput = ''
  collision.stdout.on('data', data => { collisionOutput += data })
  collision.stderr.resume()
  const collisionCode = await deadline(new Promise(resolve => collision.once('exit', resolve)), 'occupied port')
  assert.equal(collisionCode, 1, 'listener failure must fail the session')
  assert(JSON.parse(collisionOutput.trim().split('\n').at(-1)).error.length > 0)
  assert.equal((await fetch(`${origin}/missing`)).status, 404)
  const diagnostic = { token, events: [{ event: 'controller.disconnected', at: 1, sequence: 3, code: 1006 }] }
  assert.equal((await fetch(`${origin}/diagnostics`, { method: 'POST', headers: { origin }, body: JSON.stringify(diagnostic) })).status, 204,
    'controller diagnostics persist even when its WebSocket has disconnected')
  assert.equal((await fetch(`${origin}/diagnostics`, { method: 'POST', headers: { origin }, body: JSON.stringify({ ...diagnostic, token: 'wrong' }) })).status, 403)
  assert.equal((await fetch(`${origin}/diagnostics`, { method: 'POST', headers: { origin: 'https://untrusted.example' }, body: JSON.stringify(diagnostic) })).status, 403)

  await assert.rejects(connect('https://untrusted.example'))
  const denied = await connect()
  assert.equal((await request(denied, { type: 'join', token: 'wrong' })).type, 'error')
  const a = await joined(), b = await joined()
  const settings = (sequence, instrument, values = {}) => ({ type: 'settings', sequence, instrument,
    tone: 0, echo: 0, reverb: 0, warp: 0, ...values })
  assert.equal((await request(a, settings(1, 0))).accepted, true, 'instrument settings reach the synthesis kernel')
  assert.equal((await request(a, settings(1, 1))).accepted, false, 'stale settings cannot change the instrument')
  assert.equal((await request(a, settings(2, 8))).accepted, false, 'unknown instruments are rejected')
  assert.equal((await request(a, settings(2, 0, { echo: 1.1 }))).accepted, false, 'effect amounts are bounded')
  const toneStart = await request(a, touch(2, 0, 0, 0, 1))
  assert.equal(toneStart.notes, 1)
  // The tone is judged on rendered frames, not wall time: a loaded runner renders slower than real time.
  let toneEnd = await request(a, { type: 'heartbeat' })
  for (let attempt = 0; toneEnd.frames < toneStart.frames + 1000 + 9600 && attempt < 200; attempt++) {
    await delay(20)
    toneEnd = await request(a, { type: 'heartbeat' })
  }
  toneIntervals.push([toneStart.frames + 1000, toneEnd.frames, 110])
  assert.equal((await request(b, touch(1, 0, 0, .1, .5))).notes, 2)
  assert.equal((await request(a, touch(1, 1))).accepted, false, 'stale sequence rejected')
  assert.equal((await request(a, touch(3, 1, 0, 2, .5))).accepted, false, 'range rejected')
  assert.equal((await request(a, touch(4, 1, 0, .7, 1))).accepted, true)
  await delay(180)
  assert.equal((await request(a, touch(5, 2, 0, .7, 1))).notes, 1, 'one peer cannot release another')
  b.close()
  await delay(150)
  const silence = await request(a, { type: 'heartbeat' })
  assert.equal(silence.notes, 0, 'disconnect releases every owned note')
  await delay(180)
  const silenceEnd = await request(a, { type: 'heartbeat' })
  silentIntervals.push([silence.frames, silenceEnd.frames])
  assert.equal((await request(a, touch(6, 0))).accepted, true)
  await delay(1700)
  assert.equal(a.readyState, WebSocket.CLOSED, 'missed heartbeat closes peer')
  // [ORCHESTRA-NETWORK-TIMING] Native pongs sustain a live controller across delayed page timers.
  const transport = await joined(true)
  assert.equal((await request(transport, touch(1, 0))).notes, 1)
  await delay(1700)
  assert.equal(transport.readyState, WebSocket.OPEN, 'native pongs preserve a live controller without JavaScript heartbeats')
  assert.equal((await request(transport, { type: 'heartbeat' })).notes, 1, 'the held voice survives a delayed page timer')
  transport.close()
  await delay(150)
  const manual = await joined()
  let pendingPings = 0
  manual.on('ping', () => { pendingPings++ })
  for (let beat = 0; beat < 5; beat++) {
    await delay(400)
    assert.equal((await request(manual, { type: 'heartbeat' })).accepted, true)
  }
  assert.equal(pendingPings, 1, 'an application-heartbeat client cannot accumulate unanswered native pings')
  manual.pong()
  await delay(700)
  assert.equal(pendingPings, 2, 'a matching pong permits the next native probe')
  manual.close()
  await delay(150)
  const malformed = await joined()
  assert.equal((await request(malformed, touch(1, 0, 0, true, .5))).type, 'error', 'boolean is not a coordinate')
  const ensemble = []
  for (let peer = 0; peer < 8; peer++) ensemble.push(await joined())
  const full = await connect()
  assert.equal((await request(full, { type: 'join', token })).type, 'error', 'ninth controller denied')
  for (const ws of ensemble) for (let finger = 0; finger < 10; finger++) {
    assert.equal((await request(ws, touch(finger + 1, 0, finger, finger / 10, 1))).accepted, true)
  }
  assert.equal((await request(ensemble[0], { type: 'heartbeat' })).notes, 80)
  await delay(150)
  for (const ws of ensemble) ws.close()
  await delay(250)
  child.kill('SIGTERM')
  assert.equal((await deadline(exited, 'recording shutdown')).code, 0, errors + output)
  const recordedOutput = output
  const recordedDiagnostics = await readFile(`${out}/diagnostics/session-${child.pid}.jsonl`, 'utf8')
  startSession(false)
  await waitForSession()
  for (const engine of [chromium, webkit]) {
    browser = await engine.launch({ headless: !windowed })
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true })
    // Hosted browsers coalesce page timers beyond the transport's 1500ms lease.
    await context.addInitScript(() => {
      const interval = window.setInterval
      window.setInterval = (callback, milliseconds, ...args) => interval(callback, Math.max(2000, milliseconds), ...args)
    })
    const page = await context.newPage()
    const pageErrors = []
    page.on('pageerror', error => pageErrors.push(error.message))
    await page.goto(`${origin}/#${token}`)
    await joinController(page)
    await page.getByLabel('Instrument', { exact: true }).selectOption('3')
    await page.getByLabel('Touch expression', { exact: true }).selectOption('echo')
    await page.getByRole('slider', { name: 'Echo amount' }).fill('0.7')
    await page.getByLabel('Touch expression', { exact: true }).selectOption('reverb')
    await page.getByRole('slider', { name: 'Reverb amount' }).fill('0.6')
    await page.getByLabel('Touch expression', { exact: true }).selectOption('warp')
    await page.getByRole('slider', { name: 'Metallic warp amount' }).fill('0.4')
    await page.getByLabel('Touch expression', { exact: true }).selectOption('echo')
    assert.equal(await page.getByRole('slider', { name: 'Echo amount' }).inputValue(), '0.7', 'effect values survive switching expression')
    await page.getByLabel('Touch expression', { exact: true }).selectOption('volume')
    const pad = page.locator('#pad'), bounds = await pad.boundingBox()
    if (engine === chromium) {
      const cdp = await context.newCDPSession(page)
      const contacts = [0, 1].map(id => ({ id, x: bounds.x + 80 + id * 100, y: bounds.y + 120, radiusX: 4, radiusY: 4 }))
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: contacts })
      await delay(150)
      assert.equal(await observedNotes(), 2, 'phone touches reach native synthesis')
      await page.screenshot({ path: `${out}/phone-playing.png` })
      await delay(1700)
      assert.equal(await observedNotes(), 2, 'held phone touches survive idle observer lease expiry')
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: contacts.map(p => ({ ...p, y: p.y + 80 })) })
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] })
    } else { await page.touchscreen.tap(bounds.x + 100, bounds.y + 100) }
    await pad.focus()
    await page.keyboard.down('Space')
    await page.keyboard.press('ArrowRight')
    for (let beat = 0; beat < 4; beat++) {
      await delay(800)
      assert.equal(await observedNotes(), 1)
    }
    assert.match(await page.getByRole('status', { name: 'Connection status' }).innerText(), /Connected/, 'playing survives several heartbeat lease periods')
    await delay(150)
    assert.equal(await observedNotes(), 1, `${engine.name()} keyboard note plays`)
    await page.screenshot({ path: `${out}/phone-${engine.name()}-keyboard.png` })
    await page.keyboard.up('Space')
    await page.getByRole('button', { name: 'Release my notes' }).click()
    await delay(50)
    assert.equal(await observedNotes(), 0)
    await page.reload()
    await joinController(page)
    assert.deepEqual(pageErrors, [])
    await context.close()
    await browser.close(); browser = undefined
  }
  await delay(250)
  child.kill('SIGTERM')
  const result = await deadline(exited, 'native shutdown')
  assert.equal(result.code, 0, errors + output)
  const browserStopped = output.trim().split('\n').map(line => JSON.parse(line)).find(record => record.event === 'session.stopped')
  assert.equal(browserStopped.error, '')
  assert.equal(browserStopped.underruns, 0, 'browser gestures keep the audio callback supplied')
  assert(browserStopped.maxCallbackFrames >= 0 && browserStopped.maxRefillGapMs >= 0, 'audio diagnostics report callback demand and producer scheduling')
  if (windowed) assert(browserStopped.maxCallbackFrames > 0 && browserStopped.maxRefillGapMs > 0)
  assert(browserStopped.peak > .05 && browserStopped.peak <= .800001, 'phone gestures produce audible bounded synthesis')
  const records = recordedOutput.trim().split('\n').map(line => JSON.parse(line))
  const stopped = records.find(record => record.event === 'session.stopped')
  assert.equal(stopped.error, '')
  assert.equal(stopped.underruns, 0, 'audio callback always receives prepared samples')
  assert(!recordedOutput.includes(token), 'recording session secrets stay out of diagnostics')
  assert(!output.includes(token), 'session secrets stay out of diagnostics')
  const logFiles = await readdir(`${out}/diagnostics`)
  const persistent = recordedDiagnostics + await readFile(`${out}/diagnostics/session-${child.pid}.jsonl`, 'utf8')
  const events = persistent.trim().split('\n').map(line => JSON.parse(line))
  for (const event of ['session.starting', 'controller.joined', 'controller.disconnected', 'controller.expired', 'controller.rejected', 'controller.ready', 'controller.sound', 'audio.health', 'session.stopped']) {
    assert(events.some(record => record.event === event), `persistent diagnostics include ${event}`)
  }
  for (const event of ['audio.deadline_started', 'audio.deadline_stopped']) {
    assert.equal(events.filter(record => record.event === event).length, windowed ? 2 : 0,
      'each device session acquires and releases its producer scheduling policy')
  }
  assert(events.every(record => record.timestamp && record.session && record.component))
  assert(!persistent.includes(token), 'persistent logs never contain invitation secrets')
  assert(logFiles.length > 0)

  assert(stopped.peak > .05 && stopped.peak <= .800001, 'audible bounded synthesis')
  const pcm = await readFile(`${out}/controller-audio.f32`)
  assert.equal(pcm.length, stopped.frames * 4)
  for (let offset = 0; offset < pcm.length; offset += 4) {
    const sample = pcm.readFloatLE(offset)
    assert(Number.isFinite(sample) && Math.abs(sample) <= .800001)
  }
  const rate = records.find(record => record.event === 'session.starting').sampleRate
  for (const [begin, end, frequency] of toneIntervals) {
    const crossings = []
    for (let frame = begin + 1; frame < end; frame++) {
      if (pcm.readFloatLE((frame - 1) * 4) <= 0 && pcm.readFloatLE(frame * 4) > 0) crossings.push(frame)
    }
    assert(crossings.length >= 8, 'enough complete oscillator periods')
    const measured = (crossings.length - 1) * rate / (crossings.at(-1) - crossings[0])
    assert(Math.abs(measured - frequency) < 1, `pentatonic root frequency: ${measured}`)
  }
  for (const [begin, end] of silentIntervals) for (let frame = begin; frame < end; frame++) {
    assert.equal(pcm.readFloatLE(frame * 4), 0, 'released voices are silent')
  }
  for (let offset = pcm.length - 1000 * 4; offset < pcm.length; offset += 4) assert.equal(pcm.readFloatLE(offset), 0)
  console.log(JSON.stringify({ result: 'passed', mode: windowed ? 'windowed-audio-device' : 'headless', ...stopped }))
} catch (failure) {
  // What the app itself said is the evidence: a stalled reply leaves nothing else behind.
  const persisted = await readFile(`${out}/diagnostics/session-${child.pid}.jsonl`, 'utf8').catch(() => '')
  console.error(failure)
  console.error(`app stdout (tail):\n${output.slice(-4000)}\napp stderr (tail):\n${errors.slice(-4000)}\napp diagnostics (tail):\n${persisted.slice(-6000)}`)
  throw failure
} finally {
  for (const ws of clients) ws.terminate()
  await browser?.close()
  child.kill('SIGTERM')
  try { await deadline(exited, 'cleanup') }
  catch { child.kill('SIGKILL'); await exited }
}

// [ORCHESTRA-AUDIO-RECOVERY] The real device is stopped, then its configuration notification is delivered.
async function verifyDeviceRecovery() {
  const fixture = `${out}/device-recovery`
  await mkdir(fixture, { recursive: true })
  const audio = await readFile('src/toolchain/hosts/swift/orchestra/Audio.swift', 'utf8')
  await writeFile(`${fixture}/Audio.swift`, audio + `
    extension OrchestraAudio {
      var holdsTimingActivityForTest: Bool { timingActivity != nil }
      func interruptDeviceForTest() {
        engine.stop()
        NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: engine)
      }
    }
  `)
  await writeFile(`${fixture}/main.swift`, `
    import Foundation
    import AppKit
    import Darwin
    func audioPolicy() -> (active: Bool, valid: Bool) {
      var policy = thread_time_constraint_policy_data_t()
      var count = mach_msg_type_number_t(MemoryLayout.size(ofValue: policy) / MemoryLayout<integer_t>.size)
      var defaults: boolean_t = 0
      let status = withUnsafeMutableBytes(of: &policy) { bytes in
        thread_policy_get(pthread_mach_thread_np(pthread_self()), thread_flavor_t(THREAD_TIME_CONSTRAINT_POLICY),
                          bytes.bindMemory(to: integer_t.self).baseAddress!, &count, &defaults)
      }
      var clock = mach_timebase_info_data_t()
      guard status == KERN_SUCCESS, mach_timebase_info(&clock) == KERN_SUCCESS, clock.denom > 0 else { exit(1) }
      let periodNs = Double(policy.period) * Double(clock.numer) / Double(clock.denom)
      return (defaults == 0, abs(periodNs - 2_000_000) < 1000 && policy.constraint == policy.period
              && policy.computation > 0 && policy.computation <= policy.period / 2)
    }
    let checkedDeadline = DispatchSemaphore(value: 0)
    let producer = Thread {
      do {
        let deadline = OrchestraAudioDeadline()
        guard !audioPolicy().active else { exit(1) }
        try deadline.start()
        try deadline.start()
        let scheduled = audioPolicy()
        guard scheduled.active && scheduled.valid else { exit(1) }
        try deadline.stop()
        try deadline.stop()
        guard !audioPolicy().active else { exit(1) }
        checkedDeadline.signal()
      } catch { print("FAIL: audio deadline lifecycle: " + error.localizedDescription); exit(1) }
    }
    producer.qualityOfService = .userInteractive
    producer.start()
    guard checkedDeadline.wait(timeout: .now() + 5) == .success else { exit(1) }
    let audio = OrchestraAudio()
    func log(_ event: String, _ fields: [String: Any] = [:]) { print(event) }
    func performSession(_ work: @escaping () -> Void) { DispatchQueue.main.async { work() } }
    guard osprey_main() == 0, osprey_orchestra_admit(1) == 0,
          osprey_orchestra_touch(1, 1, 0, 0, 0.5, 1) == 0 else { exit(1) }
    try audio.update()
    guard !audio.holdsTimingActivityForTest else { exit(1) }
    try audio.start()
    guard audio.holdsTimingActivityForTest else { exit(1) }
    let clock = Timer(timeInterval: 0.002, repeats: true) { _ in audio.fill() }
    RunLoop.main.add(clock, forMode: .common)
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { audio.interruptDeviceForTest() }
    DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
      let recovered = audio.isRunning && audio.activeNotes == 1 && audio.renderedFrames > 20000 && audio.holdsTimingActivityForTest
      audio.stop()
      let released = !audio.holdsTimingActivityForTest
      audio.stop()
      let complete = recovered && released && !audio.holdsTimingActivityForTest
      print(complete ? "PASS: output configuration change resumes the held note and releases its timing activity" : "FAIL: audio output or timing activity lifecycle")
      exit(complete ? 0 : 1)
    }
    NSApplication.shared.run()
  `)
  runNativeFixture(fixture, `${fixture}/Audio.swift`, 10000)
}

// [ORCHESTRA-SOUND] Compare actual native PCM, independent of browser presentation and device routing.
async function verifySoundPalette() {
  const fixture = `${out}/sound-palette`
  await mkdir(fixture, { recursive: true })
  await writeFile(`${fixture}/main.swift`, `
    import Foundation
    let audio = OrchestraAudio()
    func log(_ event: String, _ fields: [String: Any] = [:]) {}
    func performSession(_ work: @escaping () -> Void) { work() }
    func check(_ value: Bool, _ message: String) {
      if !value { print("FAIL: " + message); exit(1) }
    }
    func energy(_ values: [Float]) -> Double {
      values.reduce(0) { $0 + Double(abs($1)) } / Double(values.count)
    }
    func difference(_ a: [Float], _ b: [Float]) -> Double {
      zip(a, b).reduce(0) { $0 + Double(abs($1.0 - $1.1)) } / Double(a.count)
    }
    func unusedEchoCursors() -> [Int] {
      let racks = Mirror(reflecting: audio).children.first { $0.label == "racks" }?.value as? [OrchestraEffects] ?? []
      return racks.filter { $0.peer == nil }.map { rack in
        let echo = Mirror(reflecting: rack).children.first { $0.label == "echo" }!.value
        return Mirror(reflecting: echo).children.first { $0.label == "cursor" }!.value as! Int
      }
    }
    check(osprey_main() == 0, "kernel initializes")
    check(energy(audio.render(1024)) == 0, "an empty ensemble renders exact silence")
    check(unusedEchoCursors() == Array(repeating: 0, count: 8), "idle audio leaves all eight unassigned effect racks untouched")
    check(osprey_orchestra_admit(1) == 0, "controller joins")
    var sequence: Int64 = 0
    func render(_ instrument: Int64, _ tone: Double, _ echo: Double, _ reverb: Double, _ warp: Double) throws -> [Float] {
      _ = osprey_orchestra_silence()
      try audio.update()
      _ = audio.render(1000)
      audio.clearEffects()
      sequence += 1
      check(osprey_orchestra_settings(1, sequence, instrument, tone, echo, reverb, warp) == 0, "settings accepted")
      sequence += 1
      check(osprey_orchestra_touch(1, sequence, 0, 0, 0.5, 1) == 0, "note begins")
      try audio.update()
      return audio.render(24000)
    }
    let dry = try render(0, 0, 0, 0, 0)
    check(unusedEchoCursors() == Array(repeating: 0, count: 7), "one sounding controller processes only its own rack")
    for instrument: Int64 in [1, 2, 3] {
      let result = try render(instrument, 0.6, 0, 0, 0)
      check(difference(dry, result) > 0.015, "each instrument has a distinct waveform")
      check(energy(result) > 0.015, "every instrument is audible")
    }
    for control in 0..<4 {
      let result = try render(0, control == 0 ? 1 : 0, control == 1 ? 1 : 0,
                              control == 2 ? 1 : 0, control == 3 ? 1 : 0)
      check(difference(dry, result) > 0.003, "every expression changes actual PCM")
      check(result.allSatisfy { $0.isFinite && abs($0) <= 0.8 }, "effect output stays bounded")
      _ = osprey_orchestra_silence()
      try audio.update()
      _ = audio.render(1200)
      let tail = audio.render(18000)
      if control == 1 || control == 2 { check(energy(tail) > 0.0001, "echo and reverb have audible release tails") }
      audio.clearEffects()
      check(energy(audio.render(1024)) == 0, "conductor silence clears effect memory")
    }
    check(osprey_orchestra_admit(2) == 0, "second controller joins")
    check(osprey_orchestra_settings(2, 1, 2, 0.2, 0.4, 0.6, 0.8) == 0, "independent patch accepted")
    check(osprey_orchestra_touch(2, 2, 0, 0, 0.3, 1) == 0, "second controller plays")
    try audio.update()
    check(audio.activeNotes == 1, "settings preserve controller ownership")
    let first = audio.render(4096)
    check(energy(first) > 0, "second controller produces real audio")
    check(audio.failure == nil, "no renderer faults")
    for count in [128, 512, 1024, 2048, 4096] {
      audio.fill()
      var output = [Float](repeating: 0, count: count)
      output.withUnsafeMutableBufferPointer { orchestra_ring_read($0.baseAddress, $0.count) }
      check(orchestra_ring_underruns() == 0, "prepared audio covers every supported device quantum")
      check(output.allSatisfy { $0.isFinite && abs($0) <= 0.8 }, "device quantum stays bounded")
    }
    let recording = OrchestraAudio()
    recording.capturesAudio = true
    for _ in 0..<12000 { _ = recording.render(128) }
    check(recording.failure == nil, "a full diagnostic recording never stops synthesis")
    check(!recording.capturesAudio, "diagnostic recording completes at its fixed bound")
    try recording.writeCapture(to: "${fixture}/bounded.f32")
    let captured = try Data(contentsOf: URL(fileURLWithPath: "${fixture}/bounded.f32"))
    check(captured.count == 48000 * 30 * 4, "diagnostic storage stays bounded to thirty seconds")
    check(recording.renderedFrames == 12000 * 128, "synthesis continues after recording completes")
    print("PASS: four instruments, four audible expressions, feedback tails, bounded PCM and effect clearing")
  `)
  runNativeFixture(fixture, 'src/toolchain/hosts/swift/orchestra/Audio.swift', 20000)
}

function runNativeFixture(fixture, audioSource, timeout) {
  const host = 'src/toolchain/hosts/swift/orchestra'
  execFileSync('swiftc', ['-O', '-warnings-as-errors', '-import-objc-header', `${out}/bridge.h`,
    `${host}/AudioModels.swift`, `${host}/Effects.swift`, audioSource, `${fixture}/main.swift`,
    `${out}/audio_ring.o`, `${out}/libOrchestraKernel.a`, 'src/toolchain/compiler/bin/libosprey_runtime_macos_arc.a',
    '-framework', 'AVFoundation', '-framework', 'Metal', '-framework', 'CoreGraphics', '-framework', 'AppKit',
    '-o', `${fixture}/verify`], { stdio: 'inherit', timeout: 60000 })
  execFileSync(`${fixture}/verify`, [], { stdio: 'inherit', timeout })
}

async function joinController(page) {
  await page.getByRole('button', { name: 'Join session' }).click()
  await page.getByRole('status').filter({ hasText: 'Connected' }).waitFor()
}
