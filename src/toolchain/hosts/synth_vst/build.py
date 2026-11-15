#!/usr/bin/env python3
"""[SYNTH-HOST-VST] Reproducible bounded SDK/worker build; never installs plug-ins."""
import argparse
import pathlib
import subprocess
import struct
import math
import json
import sys

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
    source = """import json, pathlib, subprocess, sys
root = pathlib.Path(__file__).parent
payload = sys.stdin.buffer.read() if sys.argv[1] == 'stream-text' else None
result = subprocess.run([WORKER, *sys.argv[1:]], input=payload, capture_output=True, timeout=15)
if payload is not None:
    path = root / ('audio-%03d.json' % len(list(root.glob('audio-*.json'))))
    path.write_text(json.dumps({'arguments':sys.argv[1:], 'input':payload.decode(), 'output':result.stdout.decode(), 'status':result.returncode}))
sys.stdout.buffer.write(result.stdout)
sys.stderr.buffer.write(result.stderr)
sys.exit(result.returncode)
"""
    script.write_text(f"#!{sys.executable}\nWORKER = {str(BUILD / 'osprey-vst-worker')!r}\n" + source)
    script.chmod(0o755)
    for old in destination.glob("audio-*.json"):
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
    records = [json.loads(path.read_text()) for path in sorted(destination.glob("audio-*.json"))]
    assert len(records) == 3 and all(record["status"] == 0 for record in records), records
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
    print("Prism MIDI -> real JX10 -> ADelay -> ADelay -> LFO-controlled stereo gain: exact VST PCM chain verified")


def prism_test(host, headless):
    destination = ROOT / "target/pr-validation" / ("prism-vst-ui-headless" if headless else "prism-vst-ui-windowed")
    destination.mkdir(parents=True, exist_ok=True)
    class_id = "0CDBB66985D548A9BFD8371909D24BB3"
    steps = [{"wait": "Scan VST3"}, {"type": ["VST3 bundle", str(BUILD / "VST3/Release/adelay.vst3")]},
             {"invoke": "Scan VST3"}, {"wait": "Choose a VST3 instrument or effect to add."},
             {"type": ["Add VST3", class_id]}, {"wait": "ADelay 0"}, {"type": ["0: Delay", "0.5"]},
             {"type": ["Add VST3", class_id]}, {"wait": "ADelay 1"}, {"invoke": "Source 0: audio"},
             {"invoke": "Connect 1: audio"}, {"wait": "Patch updated."}, {"shot": "vst-routed"},
             {"invoke": "Remove selected"}, {"gone": "ADelay 1"}, {"wait": "ADelay 0"}, {"shot": "vst-removed"}]
    audition = [{"type": ["0: Delay", "0"]}, {"type": ["1: Delay", "0"]},
                {"type": ["VST3 bundle", str(BUILD / "VST3/Release/mda-vst3.vst3")]},
                {"invoke": "Scan VST3"}, {"wait": "Choose a VST3 instrument or effect to add."},
                {"type": ["Add VST3", "5653544D44416A6D6461206A78313000"]}, {"wait": "mda JX10 2"},
                {"type": ["Add component", "midi-note"]}, {"type": ["Add component", "stereo-output"]},
                {"invoke": "Source 3: notes"}, {"invoke": "Connect 2: MIDI"},
                {"invoke": "Source 2: audio"}, {"invoke": "Connect 0: audio"},
                {"type": ["Add component", "lfo"]}, {"type": ["Add component", "modulated-gain"]},
                {"invoke": "Source 1: audio"}, {"invoke": "Connect 6: audio"},
                {"invoke": "Source 5: control"}, {"invoke": "Connect 6: control"},
                {"invoke": "Modulate 2: VCF Freq"},
                {"invoke": "Source 6: audio"}, {"invoke": "Connect 4: audio"},
                {"invoke": "Audition patch"}, {"wait": "Preview finished."}, {"shot": "vst-audition"},
                {"invoke": "Select 1"}]
    steps[13:13] = audition
    worker = recording_worker(destination)
    script = destination / "journey.json"
    script.write_text(json.dumps({"steps": steps}))
    arguments = [str(host), "--self-test", str(script), str(destination), "--live-processes",
                 "--empty", "--vst-worker", str(worker)]
    if headless:
        arguments.append("--headless")
    run(*arguments, timeout=180)
    check_prism_audio(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test", action="store_true")
    parser.add_argument("--stream-test", action="store_true")
    parser.add_argument("--prism-test", type=pathlib.Path)
    parser.add_argument("--headless", action="store_true")
    args = parser.parse_args()
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
