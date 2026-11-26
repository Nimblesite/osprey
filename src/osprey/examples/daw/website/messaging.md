# Writing about Perception Studio

This brief governs every word about **Perception Studio** (**Perception** for short): the website, README, blog posts, launch copy, and videos.

## The answer

> **Perception Studio is a transparent modular synth DAW, VST3 plug-in, and hardware synth factory — with unlimited routing, granular and spectral engines, and mathematically proven DSP.**

Second sentence, when there is room:

> **Patch any outlet to any inlet, drop Perception as a VST3 inside your favourite DAW, or flash your custom instrument straight to a hardware box.**

## Audience

**Synth nerds, sound designers, modular patchers, and hardware builders.**
Write like an engineer who builds instruments, not a marketing copywriter. Lead with signal flow, synthesis engines, and concrete specs. Cut filler. Every sentence must earn its place.

## Five pillars

1. **Unlimited routing.** Every track is a typed signal graph. Wire any module outlet to any compatible inlet, tap internal outlets straight into stereo buses, nest stereo groups with cycle protection, modulate parameters with LFOs, and host third-party VST3 instruments and effects in isolated workers.
2. **DAW and VST3.** Use Perception standalone (Timeline, Track patch, Bus routing) or load it as a **VST3 plug-in** inside Ableton, Reaper, Bitwig, Cubase, FL Studio, or any VST3 host.
3. **Mathematically proven DSP.** Core synthesis math — weighted overlap-add, traveling-wave waveguides, allpass unit magnitude, radix-2 FFT, antiderivative anti-aliasing, and macro curves — is formally proved in Lean and replayed in CI. Zero hidden limiters, clippers, or auto-gain.
4. **Hardware synth factory.** Perception is the factory; hardware boxes are profiles. Map a patch to a device profile (a 4-knob pedal, a 16-knob desktop synth, or the Raspberry Pi 5 + Perception HAT enclosure) and run the same engine on real-time Linux hardware without a desktop.
5. **Granular, physical & spectral.** 16-voice stereo grain clouds with WAV import (8–192 kHz, freeze/reverse rate, jitter), waveguide strings, bowed strings, modal bells and mallets, phase-distortion operators, formant-preserving FFT pitch shifting, phase-locked spectral freeze, and formula melodies (`60 + t * 7 % 12`).

## Rules

- **Always say Perception**, never Prism, in user-facing copy.
- **Keep text tight.** Prefer short punchy lines, concrete specs (`48 kHz`, `16-voice grain pool`, `8–192 kHz WAV`, `< 10 ms hardware latency`), and interactive UI over walls of prose.
- **No internal implementation trivia.** Do not lead with internal compiler or rendering-pipeline rules (e.g. "Master only" or stem bans); talk about what musicians can *build and hear*.
- **Honest status.** Clearly distinguish shipped studio features from in-progress hardware/VST3-export milestones without sounding apologetic.
