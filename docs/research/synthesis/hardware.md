# Prism device: hardware research [DEVICE-RESEARCH]

Collected 26 November 2026 for the [Prism device hardware](../../specs/synth/PrismDeviceHardware.md) and the [embedded target](../../specs/compiler/EmbeddedTargets.md). Each decision in those specifications cites a finding here. Prices are single-unit retail snapshots from the cited pages on this date; check them again before ordering.

## How low the latency must be [DEVICE-RESEARCH-LATENCY]

- Wessel and Wright set the accepted bound for computer instruments: **at most 10 ms** from gesture to sound, with variation (jitter) of **at most 1 ms**. They measured their own systems at about 7 ms. [Problems and Prospects for Intimate Musical Control of Computers (NIME 2001, arXiv repost)](https://arxiv.org/pdf/2010.01570)
- McPherson, Jack and Moro measured common instrument platforms against that bound and argued jitter matters as much as the mean. Their Bela platform runs synchronous audio and sensor processing with **sub-millisecond latency and jitter under 50 µs**. [Action-Sound Latency: Are Our Tools Fast Enough? (NIME 2016)](https://core.ac.uk/download/77040721.pdf), [An Environment for Submillisecond-Latency Audio and Sensor Processing on BeagleBone Black (AES 2015)](https://www.aes.org/e-lib/browse.cfm?elib=17755)
- Jack, Mehrabi, Stockman and McPherson tested percussionists and amateurs at 0 ms, 10 ms, 10 ± 3 ms and 20 ms. Both groups rated **10 ± 3 ms and 20 ms** worse than zero latency, but found **no significant difference between a steady 10 ms and zero**. Jitter, not only delay, costs quality. [Action-sound Latency and the Perceived Quality of Digital Musical Instruments (Music Perception 36(1), 2018)](https://qmro.qmul.ac.uk/xmlui/bitstream/handle/123456789/44614/Jack%20Action-sound%20Latency%202018%20Accepted.pdf?sequence=1)
- The user's requirement is the same: stay under about 10 ms. The device therefore targets **5 ms or less** for both audio round trip and note to sound, and never configures a buffer that could exceed 10 ms.

## Real-time Linux on a Raspberry Pi [DEVICE-RESEARCH-RT]

- PREEMPT_RT is in mainline Linux since **6.12**, with arm64 among the first architectures. No out-of-tree patch is needed. [Phoronix: PREEMPT_RT merged for Linux 6.12](https://phoronix.com/news/Linux-6.12-Does-Real-Time)
- On a **Raspberry Pi 5**, a 250 Hz control loop at `SCHED_FIFO` priority 99 had a worst-case latency of **224 µs under stress** on PREEMPT_RT, against 1,848 µs on the standard kernel (their Table 4; the paper measures its own loop, not `cyclictest`). [Giacomossi et al., Scheduling Analysis of UAV Flight Control Workloads on PREEMPT_RT Linux Using a Raspberry Pi 5 (arXiv 2604.19275)](https://arxiv.org/html/2604.19275) A 64-frame period at 48 kHz is 1,333 µs, so the worst case leaves most of the period for DSP.
- Xenomai, as used by Elk Audio OS, beats PREEMPT_RT on round-trip latency and usable CPU at a given buffer size on a Raspberry Pi 4 with a HiFiBerry board. [Vignati, Zambon and Turchet, AES e-library 21553](https://aes2.org/publications/elibrary-page/?id=21553) Elk claims down to 1 ms round trip; its open-source build supports only the Pi 4, and the full Xenomai OS is commercially licensed. [Elk Audio OS paper](https://people.kth.se/~carlofi/publication/96/), [Elk supported hardware](https://elk-audio.github.io/elk-docs/html/intro/supported_hw.html)
- PiPedal, an open-source Raspberry Pi guitar pedal, measured **5.8 ms** round trip at 64 frames on a Pi 4 with a USB interface and a stock kernel, and recommends a 64-bit OS (about 40% faster audio code than 32-bit) and cooling to avoid throttling above about 60–70 °C. [PiPedal: Optimizing Audio Latency](https://rerdavies.github.io/pipedal/AudioLatency.html)
- Zynthian users report about **5.2 ms** theoretical latency on a Pi 5 with the Zynthian V5 sound card. [Zynthian forum: expected latency with Raspberry Pi 5](https://discourse.zynthian.org/t/expected-latency-with-raspberry-pi-5-raspberry-pi-dac-pro-pianoteq-for-live-playing/13397)

**Decision:** Linux with the standard Raspberry Pi kernel, which is what comparable Pi instruments ship by default ([DEVICE-RESEARCH-OS]). Raspberry Pi's PREEMPT_RT kernel is the drop-in swap if measured dropouts require it. Xenomai is not used: it needs an out-of-tree kernel and, for Elk's full OS, a commercial licence.

## Operating system [DEVICE-RESEARCH-OS]

- **Raspberry Pi OS Lite (64-bit)** is the standard, vendor-supported OS for the Pi 5 and CM5, based on Debian. Zynthian, PiPedal and norns all run on Raspberry Pi OS or Debian, not a custom distribution.
- Raspberry Pi publishes `rpi-image-gen` to build customised Raspberry Pi OS images from a package list; images flash with Raspberry Pi Imager. [Raspberry Pi: introducing rpi-image-gen](https://www.raspberrypi.com/news/introducing-rpi-image-gen-build-highly-customised-raspberry-pi-software-images/), [rpi-image-gen](https://github.com/raspberrypi/rpi-image-gen)
- **What other Pi instruments run.** All of them run Linux. PiPedal runs on stock Raspberry Pi OS or Ubuntu and says real-time kernels "are preferred but not required", giving "better (but not dramatically better) latency". [PiPedal: audio latency](https://rerdavies.github.io/pipedal/AudioLatency.html) Patchbox OS, Blokas's Raspberry Pi OS for audio, boots the standard kernel by default and offers a real-time kernel from a menu. [Patchbox OS Bookworm beta](https://community.blokas.io/t/beta-patchbox-os-bookworm-arm64-2024-04-04/5163) Zynthian's forum says its images use the default preemptive kernel. [Zynthian forum: realtime kernel](https://discourse.zynthian.org/t/realtime-kernel-for-raspi/1063) Korg's wavestate, modwave and opsix are built on Raspberry Pi Compute Modules. [Raspberry Pi: Korg case study](https://www.leocom.kr/RaspberryPi/success/pdf/221031-Korg-Case-Study.pdf)
- Raspberry Pi publishes a PREEMPT_RT build of its 6.12 kernel as the package `linux-image-rpi-v8-rt`, selected in `config.txt`. [Raspberry Pi forums: RPi 6.12 kernel builds](https://forums.raspberrypi.com/viewtopic.php?t=388298), [Applying PREEMPT_RT in 2026?](https://forums.raspberrypi.com/viewtopic.php?t=396623)
- **No emulator models the device.** QEMU emulates Raspberry Pi boards only up to the 4B, with no Pi 5; Corellium's Arm Virtual Hardware has a Pi 5 model in its cloud but no model of the audio codec. Because the OS is Debian arm64, the engine itself runs unchanged in any stock Debian arm64 container or VM on an Apple-silicon Mac; audio timing needs the real prototype. [QEMU Raspberry Pi boards](https://qemu.readthedocs.io/en/master/system/arm/raspi.html), [Corellium: Raspberry Pi 5 components](https://support.avh.corellium.com/devices/rpi5/rpi5-supported-hardware-components)

**Decision:** Raspberry Pi OS Lite with its standard kernel, not Buildroot or another custom distribution: do what the other Pi instruments do. The real-time kernel stays available as a one-package swap. It is standard and simple, at the cost of a larger image and a slower boot than a minimal build.

## Prior art: open instruments on small computers [DEVICE-RESEARCH-PRIOR]

| Project | Hardware | What we take from it |
| --- | --- | --- |
| [Bela](https://opensource.com/article/18/6/bela-high-performance-audio) | BeagleBone, Xenomai, PRU-driven codec | Audio and sensors sampled in the same callback; latency and jitter measured and published |
| [Elk Audio OS](https://midi.org/elk-audio-os-for-everyone) | Raspberry Pi 4, Xenomai | A headless engine (Sushi) controlled over a network link; an appliance OS with no desktop |
| [Zynthian](https://zynthian.org/technical-specifications) | Raspberry Pi 5, I2S sound card | An open Pi synth with knobs and MIDI that ships today |
| [PiPedal](https://rerdavies.github.io/pipedal/AudioLatency.html) | Raspberry Pi 4 and 5 | A Pi effects pedal; latency and thermal measurements |
| [norns](https://monome.org/docs/norns/) and its [shield](https://github.com/monome/norns) | Raspberry Pi | An open-hardware shield that turns a stock Pi into the instrument |
| [MiniDexed](https://github.com/probonopd/MiniDexed/discussions/621) on [Circle](https://github.com/rsta2/circle/discussions/387) | Raspberry Pi, bare metal | A synth engine with no OS at all, booting straight to sound |
| [Daisy Seed](https://www.electro-smith.com/daisy/daisy) | STM32 Cortex-M7, 480 MHz, AKM codec | Sub-millisecond bare-metal audio for about USD 21–30; far less memory and CPU than a Pi |
| [Pisound](https://blokas.io/pisound/) | Raspberry Pi HAT | Audio plus DIN MIDI on one HAT; DIN MIDI loopback measured at 2.1 ms |

**Decision:** follow the norns, Zynthian and PiPedal pattern of a Raspberry Pi plus one purpose-built audio and control board, running the standard Raspberry Pi OS ([DEVICE-RESEARCH-OS]) with a headless engine controlled over a link, as Elk does. The product puts a Compute Module 5 on that board, because a Pi 5 with a HAT and cooler does not fit a 35 mm-deep pedal shell; the prototype is a stock Pi 5 with an audio HAT. Bare metal (Circle) and microcontrollers (Daisy) are later targets for the same engine ([TARGET-EMBEDDED-BARE]).

## Compute [DEVICE-RESEARCH-COMPUTE]

- The Raspberry Pi 5's Cortex-A76 cores are **2–3×** a Pi 4 in general CPU benchmarks. [Phoronix: Raspberry Pi 5 benchmarks](https://phoronix.com/review/raspberry-pi-5-benchmarks) Pianoteq, a physical-modelling piano, runs live on a Pi 5. [Blokas community: Pianoteq on Raspberry Pi 5](https://community.blokas.io/t/pianoberry-pianoteq-8-1-33-ms-latency-on-raspberry-pi-5/5627?page=2)
- No published Pi 5 polyphony benchmark exists for synth engines comparable to `@osprey/synth`. The engine's own cost table must be measured on the device ([DEVICE-ENGINE-BUDGET]).
- The hardware is fast enough for a decent stereo patch. **Osprey's engine is not yet**: it renders 0.37 s of audio per second on a Mac today, mostly in reference counting and allocation ([DAW-TRANSPORT-LIVE-SPEED]). Engine performance is therefore the most important work for the device; it proceeds in parallel with the hardware, not before it ([DEVICE-PERF]).
- **Compute Module 5 price.** Raspberry Pi raised every 4 GB product by USD 15 in February 2026 and again by USD 25 in 2026 because LPDDR4 memory costs more than doubled, so the launch list prices in the [CM5 product brief](https://pip-assets.raspberrypi.com/categories/944-raspberry-pi-compute-module-5/documents/RP-008181-DS-10-cm5-product-brief.pdf) are stale. [Raspberry Pi: more memory-driven price rises (2 Feb 2026)](https://www.raspberrypi.com/news/more-memory-driven-price-rises/), [Hackster: second 2026 rise](https://www.hackster.io/news/raspberry-pi-hikes-prices-by-up-to-150-as-the-ai-bubble-hits-hard-confirms-raspberry-pi-6-plans-711d4384a4dd) The 4 GB, 16 GB eMMC, no-wireless CM5004016 is **AUD 139.95** at Core Electronics, **out of stock** on 26 November 2026. [Core Electronics: CM5 non-wireless](https://core-electronics.com.au/raspberry-pi/boards/compute-module-5/non-wireless.html)
- **CM5 power and heat.** The CM5 datasheet gives a typical operating current of 900 mA at 5 V and throttles the SoC to keep it below 85 °C; it has less metal to sink heat than a Pi 5. [CM5 datasheet, §4.3.3 and §4.4](https://pip-assets.raspberrypi.com/categories/944-raspberry-pi-compute-module-5/documents/RP-008180-DS-8-cm5-datasheet.pdf)
- A CM5 carrier must also carry USB, power and boot circuits. That costs design effort, not unit price: the module is cheaper than a Pi 5 plus a microSD card and cooler, and it is the only option that fits the shell.

## Audio converters [DEVICE-RESEARCH-AUDIO]

- **Prototype:** HiFiBerry DAC2 ADC Pro: Burr-Brown DAC and ADC, 24-bit up to 192 kHz, 112 dB DAC and 110 dB ADC SNR, input gain −12 to +32 dB, AUD 119.90 in stock. [HiFiBerry DAC2 ADC Pro datasheet](https://www.hifiberry.com/docs/data-sheets/datasheet-dac2-adc-pro/), [shop](https://www.hifiberry.com/shop/boards/hifiberry-dac2-adc-pro)
- **Prism board:** TI PCM3060: one 24-bit stereo ADC (16–96 kHz) and DAC (16–192 kHz) in a TSSOP-28, about USD 3–6, with a mainline Linux driver. ADC and DAC are each master or slave independently. It has no crystal oscillator of its own: the system clock (SCKI) comes from an external oscillator. Group delay is **17.4/fs** for the ADC and **20/fs** for the DAC, 0.36 ms and 0.42 ms at 48 kHz. [TI PCM3060 datasheet SLAS533B](https://www.ti.com/lit/ds/symlink/pcm3060.pdf), [TI PCM3xxx Linux drivers](https://www.ti.com/tool/PCM3XXX-DRIVERS), [LCSC price](https://www.lcsc.com/product-detail/C2651841.html) Its outputs are differential, so the board carries an op-amp stage to the jacks.
- **HAT format:** Raspberry Pi HAT+ specification (2024), which replaces the 2014 HAT standard. [HAT+ specification](https://datasheets.raspberrypi.com/hat/hat-plus-specification.pdf)

## Controls and MIDI [DEVICE-RESEARCH-IO]

- MCP3208: 8-channel, 12-bit SPI ADC, 2.7–5.5 V, about USD 2. [Microchip datasheet DS21298](https://www.microchip.com/content/dam/mchp/documents/APID/ProductDocuments/DataSheets/21298e.pdf), [Octopart](https://octopart.com/part/microchip/MCP3208T-CI%2FSL)
- MIDI electrical: the 2014 update (CA-033) allows 3.3 V transmitters and adds optional RF filtering; the Pi's 3.3 V UART needs an opto-isolator that works at 3.3 V, such as the H11L1. [CA-033](https://midi.org/wp-content/uploads/wpforo/default_attachments/1709416667-ca33-MIDI-10-Electrical-Specification-Update.pdf)
- TRS MIDI: RP-054 (2018) standardises TRS; Type A is the common pinout. [Hackaday on RP-054](https://hackaday.com/2018/08/23/midi-association-releases-spec-for-trs-jacks)
- USB MIDI device: the Pi 5's USB-C port exposes the SoC's OTG controller (USB 2.0) with `dtoverlay=dwc2,dr_mode=peripheral`; the board must then be powered another way. [Raspberry Pi white paper: Using OTG mode on Raspberry Pi SBCs](https://pip-assets.raspberrypi.com/categories/685-app-notes-guides-whitepapers/documents/RP-009276-WP-1-Using%20OTG%20mode%20on%20Raspberry%20Pi%20SBCs.pdf)

- **One dual-role USB-C port.** The Linux driver for the TI TUSB320 Type-C controller reports the attached role and drives Linux's USB role switch, so one port can be USB host for a keyboard and USB device for a computer. [Linux: extcon-usbc-tusb320 usb_role_switch support (2023)](https://lkml.iu.edu/hypermail/linux/kernel/2303.2/04620.html), [Linux: TUSB320xA Type-C DRP port controller driver](https://lore.kernel.org/all/871qzcd3tw.fsf@bang-olufsen.dk/T/) The DesignWare dwc2 controller on the Pi supports host, peripheral and dual-role (`dr_mode=otg`). [Linux dwc2 binding](https://www.kernel.org/doc/Documentation/devicetree/bindings/usb/dwc2.yaml) **Risk:** the CM5 datasheet says its USB 2.0 port "can operate in USB On-The-Go (OTG) mode. While not officially documented, some users have successfully enabled this functionality"; the `USB_OTG_ID` pin selects the role and grounding it makes the CM5 a host. The TUSB320's ID output can drive that pin, but device role on a CM5 must be proven on the CM5 IO board before the Prism board is laid out. [CM5 datasheet](https://pip-assets.raspberrypi.com/categories/944-raspberry-pi-compute-module-5/documents/RP-008180-DS-6-cm5-datasheet.pdf)
- **USB MIDI 2.0.** Linux's `snd-usb-audio` supports USB MIDI 2.0 (UMP) devices and falls back to MIDI 1.0. [Linux: MIDI 2.0 on Linux](https://docs.kernel.org/sound/designs/midi-2.0.html)

## Real-time safety tooling [DEVICE-RESEARCH-RTSAFE]

- Clang's **RealtimeSanitizer** (`-fsanitize=realtime`) fails at run time when a `[[clang::nonblocking]]` function allocates, locks or makes another blocking call. Clang's **function effect analysis** uses the same attribute to find violations at compile time. [Clang: RealtimeSanitizer](https://clang.llvm.org/docs/RealtimeSanitizer.html) Osprey's C runtime and generated LLVM IR can use both, so the no-allocation callback contract ([SYNTH-RENDER]) is checked by existing tools, not by hand.

## Enclosure and prices [DEVICE-RESEARCH-PRICES]

| Part | Price | Source |
| --- | --- | --- |
| Compute Module 5, 4 GB, 16 GB eMMC, no wireless (CM5004016) | AUD 139.95 inc GST, out of stock | [Core Electronics](https://core-electronics.com.au/raspberry-pi/boards/compute-module-5/non-wireless.html) |
| Raspberry Pi 5, 4 GB | AUD 178.42 inc GST, in stock | [Core Electronics](https://core-electronics.com.au/raspberry-pi-5-model-b-4gb.html) |
| HiFiBerry DAC2 ADC Pro (prototype only) | AUD 119.90, in stock | [HiFiBerry](https://www.hifiberry.com/shop/boards/hifiberry-dac2-adc-pro) |
| TI PCM3060 | USD 3–6 | [LCSC](https://www.lcsc.com/product-detail/C2651841.html) |
| MCP3208 | about USD 2 | [Octopart](https://octopart.com/part/microchip/MCP3208T-CI%2FSL) |
| Hammond 1590XX (145 × 121 × 39 mm, IP54) | AUD 26.81 inc GST | [RS Australia](https://au.rs-online.com/web/p/general-purpose-enclosures/7568792) |

The [Hammond 1590XX drawing](https://www.hammfg.com/files/parts/pdf/1590XX.pdf) gives 145.20 × 121.20 mm outside against 139.00 × 115.00 mm inside, so the side walls are about **3.1 mm**, and 35.05 mm inside height. The same drawing lists the **1590WXX**, a liquid-resistant version with a silicone gasket, and stainless O-ring cover screws (1590WMS100). Its Australian price was not found; it is budgeted as an estimate.
