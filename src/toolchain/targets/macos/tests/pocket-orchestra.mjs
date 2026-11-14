// [ORCHESTRA-PLAN-ENSEMBLE] Real native kernel, HTTP/WebSockets, phone browser and captured PCM.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { readFile, mkdir } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
const require = createRequire(resolve('src/toolchain/website/package.json'))
const WebSocket = require('ws')
const { chromium, webkit } = require('@playwright/test')
const out = resolve('target/orchestra')
await mkdir(out, { recursive: true })
const windowed = process.env.ORCHESTRA_DEVICE_TEST === '1'
const token = 'orchestra-integration-private-token'
const port = 19140
const origin = `http://127.0.0.1:${port}`
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
const child = spawn(resolve('target/orchestra/pocket-orchestra'), [], { env: { ...process.env,
  ORCHESTRA_NO_WINDOW: windowed ? '0' : '1', ORCHESTRA_HEADLESS: windowed ? '0' : '1',
  ORCHESTRA_PORT: String(port), ORCHESTRA_TOKEN: token, ORCHESTRA_DURATION: '25',
  ORCHESTRA_CAPTURE: `${out}/controller-audio.f32` } })
let output = '', errors = '', browser
child.stdout.on('data', data => { output += data })
child.stderr.on('data', data => { errors += data })
const exited = new Promise(resolve => child.once('exit', (code, signal) => resolve({ code, signal })))
const clients = []
const silentIntervals = []
const toneIntervals = []
function deadline(promise, label) {
  let timer
  return Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(Error(`Timeout: ${label}`)), 5000) })]).finally(() => clearTimeout(timer))
}
async function connect(allowedOrigin = origin) {
  const ws = new WebSocket(`ws://127.0.0.1:${port + 1}`, 'orchestra.v1', { origin: allowedOrigin })
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
async function joined() {
  const ws = await connect()
  assert.equal((await request(ws, { type: 'join', token })).type, 'joined')
  return ws
}
try {
  for (let attempt = 0; ; attempt++) {
    try { assert.equal((await fetch(origin)).status, 200); break }
    catch (error) { if (attempt === 60) throw error; await delay(50) }
  }
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
  await assert.rejects(connect('https://untrusted.example'))
  const denied = await connect()
  assert.equal((await request(denied, { type: 'join', token: 'wrong' })).type, 'error')
  const a = await joined(), b = await joined()
  const toneStart = await request(a, touch(1, 0, 0, 0, 1))
  assert.equal(toneStart.notes, 1)
  await delay(180)
  const toneEnd = await request(a, { type: 'heartbeat' })
  toneIntervals.push([toneStart.frames + 1000, toneEnd.frames, 110])
  assert.equal((await request(b, touch(1, 0, 0, .1, .5))).notes, 2)
  assert.equal((await request(a, touch(1, 1))).accepted, false, 'stale sequence rejected')
  assert.equal((await request(a, touch(2, 1, 0, 2, .5))).accepted, false, 'range rejected')
  assert.equal((await request(a, touch(3, 1, 0, .7, 1))).accepted, true)
  await delay(180)
  assert.equal((await request(a, touch(4, 2, 0, .7, 1))).notes, 1, 'one peer cannot release another')
  b.close()
  await delay(150)
  const silence = await request(a, { type: 'heartbeat' })
  assert.equal(silence.notes, 0, 'disconnect releases every owned note')
  await delay(180)
  const silenceEnd = await request(a, { type: 'heartbeat' })
  silentIntervals.push([silence.frames, silenceEnd.frames])
  assert.equal((await request(a, touch(5, 0))).accepted, true)
  await delay(1700)
  assert.equal(a.readyState, WebSocket.CLOSED, 'missed heartbeat closes peer')
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
  for (const engine of [chromium, webkit]) {
    browser = await engine.launch({ headless: !windowed })
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true })
    const page = await context.newPage()
    const pageErrors = []
    page.on('pageerror', error => pageErrors.push(error.message))
    await page.goto(`${origin}/#${token}`)
    await page.getByRole('button', { name: 'Join session' }).click()
    await page.getByRole('status').filter({ hasText: 'Connected' }).waitFor()
    const observer = await joined()
    const pad = page.locator('#pad'), bounds = await pad.boundingBox()
    if (engine === chromium) {
      const cdp = await context.newCDPSession(page)
      const contacts = [0, 1].map(id => ({ id, x: bounds.x + 80 + id * 100, y: bounds.y + 120, radiusX: 4, radiusY: 4 }))
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: contacts })
      await delay(150)
      assert.equal((await request(observer, { type: 'heartbeat' })).notes, 2, 'phone touches reach native synthesis')
      await page.screenshot({ path: `${out}/phone-playing.png` })
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: contacts.map(p => ({ ...p, y: p.y + 80 })) })
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] })
    } else { await page.touchscreen.tap(bounds.x + 100, bounds.y + 100) }
    await pad.focus()
    await page.keyboard.down('Space')
    await page.keyboard.press('ArrowRight')
    await delay(150)
    assert.equal((await request(observer, { type: 'heartbeat' })).notes, 1, `${engine.name()} keyboard note plays`)
    await page.screenshot({ path: `${out}/phone-${engine.name()}-keyboard.png` })
    await page.keyboard.up('Space')
    await page.getByRole('button', { name: 'Release my notes' }).click()
    await delay(50)
    assert.equal((await request(observer, { type: 'heartbeat' })).notes, 0)
    await page.reload()
    await page.getByRole('button', { name: 'Join session' }).click()
    await page.getByRole('status').filter({ hasText: 'Connected' }).waitFor()
    assert.deepEqual(pageErrors, [])
    observer.close()
    await context.close()
    await browser.close(); browser = undefined
  }
  await delay(250)
  child.kill('SIGTERM')
  const result = await deadline(exited, 'native shutdown')
  assert.equal(result.code, 0, errors + output)
  const records = output.trim().split('\n').map(line => JSON.parse(line))
  const stopped = records.find(record => record.event === 'session.stopped')
  assert.equal(stopped.error, '')
  assert.equal(stopped.underruns, 0, 'audio callback always receives prepared samples')
  assert(!output.includes(token), 'session secrets stay out of diagnostics')
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
} finally {
  for (const ws of clients) ws.terminate()
  await browser?.close()
  child.kill('SIGTERM')
  await deadline(exited, 'cleanup')
}
