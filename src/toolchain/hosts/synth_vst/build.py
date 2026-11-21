#!/usr/bin/env python3
"""[SYNTH-HOST-VST] Reproducible bounded SDK/worker build; never installs plug-ins."""
import argparse
import pathlib
import subprocess
import struct
import math
import json
import sys
import os
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = pathlib.Path(__file__).resolve().parents[4]
SDK = ROOT / "target/vst3sdk"
BUILD = ROOT / "target/vst-worker"
REVISION = "3cdf9ca5d1f5b1b21e0a86832aa4abe55607bd96"
SOURCE = "https://github.com/steinbergmedia/vst3sdk.git"


def run(*args, timeout=180):
    subprocess.run(args, cwd=ROOT, check=True, timeout=timeout)


def sdk():
    SDK.parent.mkdir(parents=True, exist_ok=True)
    if not SDK.exists():
        run("git", "clone", "--filter=blob:none", "--no-checkout", SOURCE, str(SDK))
        run("git", "-C", str(SDK), "checkout", "--detach", REVISION)
    actual = subprocess.check_output(
        ["git", "-C", str(SDK), "rev-parse", "HEAD"], text=True, timeout=10
    ).strip()
    if actual != REVISION:
        raise RuntimeError(f"SDK revision mismatch: expected {REVISION}, found {actual}")
    dirty = subprocess.check_output(
        ["git", "-C", str(SDK), "status", "--porcelain", "--untracked-files=no"],
        text=True, timeout=10)
    if dirty.strip():
        raise RuntimeError("Pinned SDK checkout has modified tracked files")
    run("git", "-C", str(SDK), "submodule", "update", "--init", "--depth", "1",
        "base", "cmake", "pluginterfaces", "public.sdk")


def packet(frames, inputs=(), events=(), parameters=()):
    return (struct.pack("<III", frames, len(events), len(parameters)) +
            b"".join(struct.pack("<IIIIIf", *event) for event in events) +
            b"".join(struct.pack("<IId", *parameter) for parameter in parameters) +
            struct.pack(f"<{len(inputs)}f", *inputs))


def streamed(bundle, identifier, payload, valid=True, mode="stream"):
    if mode == "stream-text":
        payload = payload.hex().encode()
    result = subprocess.run([str(BUILD / "osprey-vst-worker"), mode, str(bundle), identifier],
                            input=payload, capture_output=True, timeout=15)
    if not valid:
        assert result.returncode != 0 and b"vst.failure" in result.stderr, result
        return []
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    output = bytes.fromhex(result.stdout.decode()) if mode == "stream-text" else result.stdout
    return decoded_audio(output)


def decoded_audio(output):
    magic, inputs, outputs, notes = struct.unpack_from("<4sIII", output)
    assert magic == b"OSV1" and outputs == 2 and inputs <= 2 and notes <= 1
    position, blocks = 16, []
    while position < len(output):
        frames, latency = struct.unpack_from("<II", output, position)
        assert 0 < frames <= 256 and latency < 480000
        position += 8
        values = struct.unpack_from(f"<{frames * outputs}f", output, position)
        assert all(math.isfinite(value) and abs(value) <= 1000000 for value in values)
        blocks.append(values)
        position += frames * outputs * 4
    assert position == len(output)
    return blocks


def session_audio(bundle, identifier, packets):
    """Each response must arrive before stdin closes; one plug-in owns every transaction."""
    with subprocess.Popen([str(BUILD / "osprey-vst-worker"), "stream-session", str(bundle), identifier],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child, ThreadPoolExecutor(max_workers=1) as reader:
        try:
            blocks = []
            for payload in packets:
                child.stdin.write(payload.hex().encode() + b"\n")
                child.stdin.flush()
                response = reader.submit(child.stdout.readline, 4 * 1024 * 1024 + 1).result(timeout=5)
                assert response.endswith(b"\n") and len(response) <= 4 * 1024 * 1024, "Incomplete or excessive session response"
                blocks.extend(decoded_audio(bytes.fromhex(response.decode())))
            child.stdin.close()
            assert child.wait(timeout=5) == 0, child.stderr.read().decode(errors="replace")
            return blocks
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)



def stream_tests():
    effect = BUILD / "VST3/Release/adelay.vst3"
    instrument = BUILD / "VST3/Release/mda-vst3.vst3"
    delay_id, synth_id = "0CDBB66985D548A9BFD8371909D24BB3", "5653544D44416A6D6461206A78313000"
    inspected = subprocess.run([str(BUILD / "osprey-vst-worker"), "describe", str(effect), delay_id],
                               capture_output=True, text=True, timeout=15, check=True)
    descriptor = json.loads(inspected.stdout)
    assert descriptor["id"] == delay_id and descriptor["inputs"] == 2 and descriptor["outputs"] == 2
    assert descriptor["notes"] is False and descriptor["name"] == "ADelay"
    delay = next(p for p in descriptor["parameters"] if p["id"] == 100)
    assert delay["name"] == "Delay" and delay["initial"] == 1.0 and delay["automatable"] is True
    # ADelay documents a one-sample minimum; impulses cross the block boundary.
    samples = [0.25, -0.125] + [0.0] * 508 + [0.5, -0.25]
    audio = streamed(effect, delay_id, packet(256, samples, parameters=[(100, 0, 0.0)]) + packet(128, [0.0] * 256))
    assert len(audio) == 2 and list(audio[0]) == [0.0, 0.0] + samples[:-2]
    assert list(audio[1]) == [0.5, -0.25] + [0.0] * 254
    text_audio = streamed(effect, delay_id, packet(256, samples, parameters=[(100, 0, 0.0)]) + packet(128, [0.0] * 256), mode="stream-text")
    assert text_audio == audio
    assert session_audio(effect, delay_id, [packet(256, samples, parameters=[(100, 0, 0.0)]), packet(128, [0.0] * 256)]) == audio
    dense_parameters = [(100, 0, 0.0)] * 1024
    assert len(streamed(effect, delay_id, packet(256, [0.0] * 512, parameters=dense_parameters))) == 1
    streamed(effect, delay_id, packet(256, [0.0] * 512, parameters=dense_parameters + [(100, 0, 0.0)]), False)
    streamed(instrument, synth_id, packet(256) * 129, False, "stream-text")
    # One instance retains the held note through many blocks, then receives its note-off.
    note = (1, 17, 60, 0, 42, 0.8)
    release = (2, 0, 60, 0, 42, 0.0)
    payload = packet(256, events=[note], parameters=[(18, 0, 0.0)]) + packet(256) * 20 + packet(256, events=[release]) + packet(256) * 100
    voice = streamed(instrument, synth_id, payload)
    assert len(voice) == 122 and max(map(abs, voice[10])) > 0.001
    assert not any(voice[0][:34]) and max(map(abs, voice[-1])) == 0
    held = streamed(instrument, synth_id, packet(256, events=[note], parameters=[(18, 0, 0.0)]) + packet(256) * 121)
    assert max(map(abs, held[-1])) > 0.001
    transactions = [packet(256, events=[note], parameters=[(18, 0, 0.0)])] + [packet(256)] * 20 + [packet(256, events=[release])] + [packet(256)] * 140
    continued = session_audio(instrument, synth_id, transactions)
    assert continued == streamed(instrument, synth_id, b"".join(transactions))
    for invalid in (b"\n", b"0\n", b"GG\n", packet(256).hex().encode(), b"00" * (1024 * 1024 + 1) + b"\n"):
        result = subprocess.run([str(BUILD / "osprey-vst-worker"), "stream-session", str(instrument), synth_id],
                                input=invalid, capture_output=True, timeout=15)
        assert result.returncode != 0 and b"vst.failure" in result.stderr and not result.stdout
    assert max(map(abs, continued[10])) > 0.001 and max(map(abs, continued[-1])) == 0
    chord = [(1, 0, 48 + i % 24, 0, i, 0.5) for i in range(128)]
    assert len(streamed(instrument, synth_id, packet(256, events=chord))) == 1
    for invalid in (packet(0), packet(257), b"short", packet(256, events=[(1, 256, 60, 0, 42, .8)]),
                    packet(256, events=[(1, 0, 128, 0, 42, .8)]), packet(256, parameters=[(0, 0, float("nan"))]),
                    packet(256, events=chord + [note]), packet(256, events=[note, (2, 0, 60, 0, 42, 0.0)]),
                    packet(256, parameters=[(18, 20, .5), (18, 0, .5)]), packet(256, parameters=[(999999, 0, .5)])):
        streamed(instrument, synth_id, invalid, False)
    streamed(effect, delay_id, packet(256, [float("nan")] + [0.0] * 511), False)
    print("Persistent VST audio, note lifecycle, automation and malformed-frame journeys passed")


def recording_worker(destination):
    """Capture the actual worker boundary for independent PCM and MIDI assertions."""
    script = destination / "record-worker.py"
    fixture_folder = destination / "plugins"
    fixture_folder.mkdir(exist_ok=True)
    for name in ("adelay.vst3", "mda-vst3.vst3"):
        link = fixture_folder / name
        if not link.exists():
            link.symlink_to(BUILD / "VST3/Release" / name, target_is_directory=True)
    source = """import json, os, pathlib, subprocess, sys, threading, time
root = pathlib.Path(__file__).parent
if sys.argv[1] == 'list':
    sys.argv[2] = str(root / 'plugins')
if sys.argv[1] in ('stream-session', 'audio-device'):
    child = subprocess.Popen([WORKER, *sys.argv[1:]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    def diagnostics():
        for line in child.stderr:
            sys.stderr.buffer.write(line)
            sys.stderr.buffer.flush()
    threading.Thread(target=diagnostics, daemon=True).start()
    try:
        for index, payload in enumerate(sys.stdin.buffer):
            child.stdin.write(payload)
            child.stdin.flush()
            output = child.stdout.readline(4194306)
            if not output.endswith(b'\\n'):
                raise RuntimeError('Worker ended before a bounded response')
            record = {'arguments':sys.argv[1:], 'input':payload.decode().strip(),
                      'output':output.decode().strip(), 'status':0, 'session':os.getpid(), 'worker':child.pid, 'order':time.monotonic_ns()}
            prefix = 'audio' if sys.argv[1] == 'stream-session' else 'device'
            (root / ('%s-%s-%05d.json' % (prefix, os.getpid(), index))).write_text(json.dumps(record))
            sys.stdout.buffer.write(output)
            sys.stdout.buffer.flush()
    finally:
        child.stdin.close()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
else:
    result = subprocess.run([WORKER, *sys.argv[1:]], capture_output=True, timeout=15)
    sys.stdout.buffer.write(result.stdout)
    sys.stderr.buffer.write(result.stderr)
    sys.exit(result.returncode)
"""
    script.write_text(f"#!{sys.executable}\nWORKER = {str(BUILD / 'osprey-vst-worker')!r}\n" + source)
    script.chmod(0o755)
    for pattern in ["audio-*.json", "device-*.json"]:
        for old in destination.glob(pattern):
            old.unlink()
    return script


def input_blocks(record):
    payload = bytes.fromhex(record["input"])
    inputs = struct.unpack_from("<I", bytes.fromhex(record["output"]), 4)[0]
    position, first = 0, 0
    while position < len(payload):
        frames, note_count, parameter_count = struct.unpack_from("<III", payload, position)
        position += 12
        notes = payload[position:position + note_count * 24]
        position += note_count * 24
        parameters = payload[position:position + parameter_count * 16]
        position += parameter_count * 16
        audio = payload[position:position + frames * inputs * 4]
        position += frames * inputs * 4
        yield first, frames, notes, parameters, audio
        first += frames
    assert position == len(payload) and first == 12288


def recorded_events(record):
    notes, parameters = [], []
    for first, _, note_bytes, parameter_bytes, _ in input_blocks(record):
        for kind, offset, pitch, channel, identity, velocity in struct.iter_unpack("<IIIIIf", note_bytes):
            notes.append((kind, first + offset, pitch, channel, identity, velocity))
        for identity, offset, value in struct.iter_unpack("<IId", parameter_bytes):
            parameters.append((identity, first + offset, value))
    return notes, parameters


def stationary_control(record, identity):
    """Keep the same initial setting while removing only subsequent modulation points."""
    blocks = []
    for first, frames, notes, parameter_bytes, audio in input_blocks(record):
        parameters = [struct.pack("<IId", parameter, offset, value)
                      for parameter, offset, value in struct.iter_unpack("<IId", parameter_bytes)
                      if parameter != identity or first + offset == 0]
        blocks.append(struct.pack("<III", frames, len(notes) // 24, len(parameters)) + notes + b"".join(parameters) + audio)
    return b"".join(blocks)


def check_prism_audio(destination):
    transactions = sorted((json.loads(path.read_text()) for path in destination.glob("audio-*.json")), key=lambda r: r["order"])
    sessions = {}
    for record in transactions:
        sessions.setdefault(record["session"], []).append(record)
    assert len(sessions) == 3 and all(len(blocks) == 6 for blocks in sessions.values()), "Expected three retained workers, each processing six graph blocks"
    records = []
    for blocks in sessions.values():
        assert all(block["status"] == 0 and block["arguments"] == blocks[0]["arguments"] for block in blocks)
        records.append({**blocks[0], "input": "".join(block["input"] for block in blocks),
                        "output": blocks[0]["output"][:32] + "".join(block["output"][32:] for block in blocks)})
    samples = [[sample for block in decoded_audio(bytes.fromhex(record["output"])) for sample in block]
               for record in records]
    events, parameters = recorded_events(records[0])
    modulation = [(offset, value) for identity, offset, value in parameters if identity == 6]
    assert [offset for offset, _ in modulation] == list(range(0, 12288, 128))
    assert modulation[0][1] == 0.5 and max(value for _, value in modulation) > 0.95
    assert min(value for _, value in modulation) < 0.5
    assert [(kind, offset, pitch, channel, identity) for kind, offset, pitch, channel, identity, _ in events] == [(1, 0, 60, 0, 3), (2, 6000, 60, 0, 3)]
    assert abs(events[0][5] - 0.8) < 1e-6 and events[1][5] == 0.0
    assert all(len(audio) == 12288 * 2 for audio in samples)
    assert max(abs(value) for value in samples[0]) > 0.01, "The real VST instrument is silent"
    instrument = records[0]["arguments"]
    baseline = [sample for block in streamed(instrument[1], instrument[2], stationary_control(records[0], 6)) for sample in block]
    assert max(abs(actual - fixed) for actual, fixed in zip(samples[0], baseline, strict=True)) > 0.001, "VST modulation did not change the rendered audio"
    assert samples[1] == [0.0, 0.0] + samples[0][:-2], "First effect did not receive instrument PCM"
    assert samples[2] == [0.0, 0.0] + samples[1][:-2], "Second effect did not receive first effect PCM"
    device = sorted((json.loads(path.read_text()) for path in destination.glob("device-*.json")), key=lambda r: r["order"])
    assert len(device) == 7 and len({r["session"] for r in device}) == 1, "One retained device must receive six blocks and a drain"
    receipts = [json.loads(r["output"]) for r in device]
    assert [r["acceptedFrames"] for r in receipts] == [2048, 4096, 6144, 8192, 10240, 12288, 12288]
    assert receipts[-1]["returnedFrames"] == 12288 and receipts[-1]["queuedFrames"] == 0 and not receipts[-1]["running"]
    assert all(r["emptyCallbacks"] == 0 for r in receipts), "The device queue starved during the real graph journey"
    print("Prism MIDI -> real JX10 -> ADelay -> ADelay -> LFO-controlled stereo gain: exact VST PCM chain verified")


def prism_test(host, headless):
    destination = ROOT / "target/pr-validation" / ("prism-vst-ui-headless" if headless else "prism-vst-ui-windowed")
    destination.mkdir(parents=True, exist_ok=True)
    class_id = "0CDBB66985D548A9BFD8371909D24BB3"
    steps = [{"wait": "ADelay"}, {"type": ["Add component", "vst:" + class_id]}, {"wait": "ADelay 0"},
             {"type": ["Module", "0"]}, {"invoke": "ADelay 0 Delay"}, {"type": ["Delay", "0.5"]},
             {"type": ["Add component", "vst:" + class_id]}, {"wait": "ADelay 1"},
             {"invoke": "Canvas output 0:0"}, {"invoke": "Canvas input 1:0"}, {"wait": "Cable 0:0 to 1:0"},
             {"drag": ["Canvas ADelay 0", "20", "130"]}, {"shot": "vst-routed"},
             {"context": "Canvas ADelay 1"}, {"invoke": "Delete node"}, {"gone": "ADelay 1"}, {"wait": "ADelay 0"}, {"shot": "vst-removed"}]
    audition = [{"type": ["Module", "0"]}, {"invoke": "ADelay 0 Delay"}, {"type": ["Delay", "0"]}, {"type": ["Module", "1"]}, {"invoke": "ADelay 1 Delay"}, {"type": ["Delay", "0"]},
                {"type": ["Add component", "vst:5653544D44416A6D6461206A78313000"]}, {"wait": "mda JX10 2"},
                {"type": ["Add component", "midi-note"]}, {"type": ["Add component", "stereo-output"]},
                {"invoke": "Canvas output 3:0"}, {"invoke": "Canvas input 2:0"},
                {"invoke": "Canvas output 2:0"}, {"invoke": "Canvas input 0:0"},
                {"type": ["Add component", "lfo"]}, {"type": ["Add component", "modulated-gain"]},
                {"invoke": "Canvas output 1:0"}, {"invoke": "Canvas input 6:0"},
                {"invoke": "Canvas output 5:0"}, {"invoke": "Canvas input 6:1"},
                {"type": ["Module", "2"]}, {"invoke": "Modulate VCF Freq"},
                {"invoke": "Canvas output 6:0"}, {"invoke": "Canvas input 4:0"},
                {"invoke": "Audition Patch"}, {"wait": "Preview finished."}, {"shot": "vst-audition"},
                {"type": ["Module", "1"]}]
    steps.insert(0, {"invoke": "Patch Controls"})
    before_remove = steps.index({"context": "Canvas ADelay 1"})
    steps[before_remove:before_remove] = audition
    worker = recording_worker(destination)
    script = destination / "journey.json"
    script.write_text(json.dumps({"steps": steps}))
    arguments = [str(host), "--self-test", str(script), str(destination), "--live-processes",
                 "--empty", "--vst-worker", str(worker)]
    if headless:
        arguments.extend(["--headless", "--defer-processes"])
    run(*arguments, timeout=180)
    check_prism_audio(destination)
    live = destination / "live"
    live.mkdir(exist_ok=True)
    live_worker = recording_worker(live)
    live_steps = steps[:steps.index({"invoke": "Audition Patch"})] + [
        {"invoke": "Play Patch"}, {"wait": "Playing the patch."}, {"frames": "480"},
        {"invoke": "Stop Patch"}, {"wait": "Playback stopped."}, {"frames": "8"}, {"shot": "live-stopped"}]
    script = live / "journey.json"
    script.write_text(json.dumps({"steps": live_steps}))
    arguments[arguments.index("--self-test") + 1:arguments.index("--self-test") + 3] = [str(script), str(live)]
    arguments[arguments.index("--vst-worker") + 1] = str(live_worker)
    run(*arguments, timeout=180)
    check_prism_live(live)


def check_prism_live(destination):
    records = [json.loads(path.read_text()) for path in destination.glob("*-*.json") if path.name.startswith(("audio-", "device-"))]
    devices = sorted((r for r in records if r["arguments"] == ["audio-device"]), key=lambda r: r["order"])
    receipts = [json.loads(r["output"]) for r in devices]
    assert len({r["session"] for r in records}) == 4, "Live playback must retain three VSTs and one device"
    assert receipts and receipts[-1]["acceptedFrames"] >= 48000, "Live playback stopped at the audition boundary"
    assert all(r["running"] for r in receipts[1:]), "The device stopped before the Stop action"
    assert all(r["emptyCallbacks"] == 0 for r in receipts), "Live graph production starved the device queue"
    identities = {r[key] for r in records for key in ["session", "worker"]}
    deadline = time.monotonic() + 3
    while identities and time.monotonic() < deadline:
        for identity in list(identities):
            try:
                os.kill(identity, 0)
            except ProcessLookupError:
                identities.remove(identity)
        if identities:
            time.sleep(.02)
    assert not identities, f"Stop or host shutdown left owned workers alive: {identities}"
    print(f"Prism sustained {receipts[-1]['acceptedFrames']} frames through real VSTs/device, then stopped all owned workers")


def device_tests():
    """Exercise the actual output queue and reject malformed PCM before enqueueing."""
    def wave(frames, channels=2, sample=0.01):
        audio = struct.pack(f"<{frames * channels}f", *([sample] * frames * channels))
        return (b"RIFF" + struct.pack("<I", 50 + len(audio)) + b"WAVEfmt " +
                struct.pack("<IHHIIHHH", 18, 3, channels, 48000, 48000 * channels * 4, channels * 4, 32, 0) +
                b"fact" + struct.pack("<II", 4, frames) + b"data" + struct.pack("<I", len(audio)) + audio)
    with subprocess.Popen([str(BUILD / "osprey-vst-worker"), "audio-device"], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child, ThreadPoolExecutor(max_workers=1) as reader:
        try:
            for index, payload in enumerate([wave(2048), wave(2048), wave(2048)]):
                child.stdin.write(payload.hex().encode() + b"\n")
                child.stdin.flush()
                receipt = json.loads(reader.submit(child.stdout.readline, 4096).result(timeout=5))
                assert receipt["status"] == 0 and receipt["acceptedFrames"] == (index + 1) * 2048, receipt
                assert receipt["queuedFrames"] <= 6144 and receipt["channels"] == 2, receipt
            time.sleep(.25)
            child.stdin.write(wave(256).hex().encode() + b"\n")
            child.stdin.flush()
            receipt = json.loads(reader.submit(child.stdout.readline, 4096).result(timeout=5))
            assert receipt["emptyCallbacks"] > 0 and receipt["acceptedFrames"] == 6400, receipt
            child.stdin.write(b"!drain\n")
            child.stdin.flush()
            receipt = json.loads(reader.submit(child.stdout.readline, 4096).result(timeout=5))
            assert receipt["returnedFrames"] == 6400 and receipt["queuedFrames"] == 0, receipt
            assert receipt["running"] is False, receipt
        finally:
            child.kill()
            child.wait(timeout=5)
        assert b"audio.queue_empty" in child.stderr.read(), "Queue starvation needs a structured backend diagnostic"
    for payload in [b"short", wave(1, sample=float("nan")), wave(1, sample=float("inf")),
                    wave(1, sample=1.01), wave(0), wave(8193), wave(1, 3), wave(1) + b"trailing"]:
        result = subprocess.run([str(BUILD / "osprey-vst-worker"), "audio-device"],
                                input=payload.hex().encode() + b"\n", capture_output=True, timeout=5)
        assert result.returncode != 0 and b"audio.failure" in result.stderr and not result.stdout, result
    print("Actual audio queue: ordered buffers, drain, diagnostics and malformed PCM rejection passed")


def prism_design(host, headless):
    """[PRISM-ROUTING-PRESENTATION] Real module insertion, cables and independently docked keys."""
    folder = ROOT / "target/pr-validation" / ("prism-design-headless" if headless else "prism-design-window")
    folder.mkdir(parents=True, exist_ok=True)
    script = ROOT / "src/osprey/examples/daw/self-test.json"
    run(str(host), "--machine", str(ROOT / "src/osprey/packages/signal_flow/signal_flow.machine.json"),
        "--self-test", str(script), str(folder), *(["--headless"] if headless else []))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--device-test", action="store_true")
    parser.add_argument("--test", action="store_true")
    parser.add_argument("--stream-test", action="store_true")
    parser.add_argument("--prism-test", type=pathlib.Path)
    parser.add_argument("--design-test", type=pathlib.Path)
    parser.add_argument("--headless", action="store_true")
    args = parser.parse_args()
    if args.design_test:
        prism_design(args.design_test, args.headless)
        return
    if args.device_test:
        device_tests()
        return
    if args.prism_test:
        prism_test(args.prism_test, args.headless)
        return
    if args.stream_test:
        stream_tests()
        return
    sdk()
    run("cmake", "-S", str(pathlib.Path(__file__).parent), "-B", str(BUILD),
        f"-DVST3_SDK={SDK}", "-DCMAKE_BUILD_TYPE=Release")
    targets = ["osprey-vst-worker", "adelay", "mda-vst3"] if args.test else ["osprey-vst-worker"]
    run("cmake", "--build", str(BUILD), "--parallel", "4", "--target", *targets, timeout=600)
    if args.test:
        run("ctest", "--test-dir", str(BUILD), "--output-on-failure", timeout=120)


if __name__ == "__main__":
    main()
