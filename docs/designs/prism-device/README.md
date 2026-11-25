# Prism device designs [DEVICE]

Designs for the reference Prism device and its face in Prism. See [Prism device hardware](../../specs/synth/PrismDeviceHardware.md), the [Instrument builder](../../specs/synth/PrismInstrument.md) and the [typed device model](device.td).

## Top panel [DEVICE-HW-CONTROLS]

The front edge faces the player, with the footswitches closest and the knobs above them.

| Row | Controls |
| --- | --- |
| Top | Four knobs, K1–K4 |
| Second | Four knobs, K5–K8 |
| Third | Two two-way toggles (S1, S2) and two three-way toggles (S3, S4), with the RGB status LED between them |
| Bottom | Left and right footswitches with their LEDs |

The printed overlay ([PRISM-INSTRUMENT-DEVICE-LABELS]) puts each macro's name under its control. Use a charcoal shell, the PRISM wordmark and high-contrast control and connector legends. Leave the area between the footswitches plain.

## Enclosure [DEVICE-HW-ENCLOSURE]

![Top, rear and both sides of the Prism enclosure](enclosure.svg)

The Hammond 1590XX die-cast aluminium shell measures 145.2 × 121.2 × 39.3 mm with the bottom lid and has about 35.05 mm nominal internal height. Check the complete CM5 carrier and controls against the casting taper, corner bosses and lid lip. The deeper 1590X keeps the same footprint and provides about 52.4 mm internal height if the selected parts need more clearance. The [panel layout](enclosure.svg) gives nominal centres, 61.2 mm footswitch spacing and connector order. Control and connector apertures depend on the selected parts. [1590XX mechanical drawing](https://www.hammfg.com/files/parts/pdf/1590XX.pdf) · [1590X mechanical drawing](https://www.hammfg.com/files/parts/pdf/1590X.pdf)

| Face (player's orientation) | Hardware and access |
| --- | --- |
| Top | Two rows of four low-profile 10 kΩ linear pots, four panel toggles, central RGB status light, two soft-touch momentary footswitches and their indicator LEDs. The shell takes the footswitch load through washers and locknuts. |
| Left | ¼-inch L/R line outputs and the amplified 3.5 mm stereo headphones. |
| Right | ¼-inch L/R inputs and separate recessed L/R line/instrument switches. Check switch access with both inputs already plugged in. |
| Rear, looking at the connectors | Insulated 2.1 mm centre-negative DC barrel, USB-C device with MIDI, the Prism link and optional input power, USB-A MIDI host, 3.5 mm Type A TRS MIDI in and out. Check access with all five cables connected, including right-angle plugs. |
| Bottom | Removable lid, non-slip feet and serial label. The factory jig reaches the knob wiper header and eMMC provisioning strap with the lid open. |

### Connecting a MIDI keyboard

A keyboard can play Prism directly in sound-source mode. Use the cable that matches the keyboard's MIDI output.

| Keyboard connection | Cable to Prism |
| --- | --- |
| Class-compliant USB MIDI | A USB data cable from Prism's **USB-A HOST** to the keyboard's USB device port, often USB-B or USB-C. The keyboard is powered from the USB host port within its current budget, or from its own supply. |
| 5-pin DIN MIDI OUT | A 5-pin MIDI cable and a **3.5 mm TRS MIDI Type A** adapter into Prism's **MIDI IN**. |
| 3.5 mm TRS MIDI Type A OUT | A straight-through 3.5 mm TRS cable into Prism's **MIDI IN**. A Type B keyboard uses an A-to-B adapter or cable. |

When a computer plays Prism, connect it to **USB-C DEVICE** with a data cable. The cable carries MIDI and the Prism link. Use the barrel supply when the USB-C source cannot power Prism and the attached keyboard. [MIDI Association USB MIDI guide](https://midi.org/basic-of-usb) · [TRS MIDI specification](https://midi.org/specification-for-trs-adapters-adopted-and-released)

Mount the CM5 carrier on a removable internal bracket, with the module facing the shell and the footswitch volumes at the player end clear. Use restrained and removable harnesses, keep cable insertion loads on the shell or bracket, and leave room for harness bends, fasteners and tool access. The Pi 5 and HiFiBerry DAC2 ADC Pro are the bench prototype. [CM5 mechanical and thermal data](https://pip-assets.raspberrypi.com/categories/944-raspberry-pi-compute-module-5/documents/RP-008180-DS-8-cm5-datasheet.pdf) · [HiFiBerry prototype board](https://www.hifiberry.com/docs/data-sheets/datasheet-dac2-adc-pro/)

Use a supported, electrically isolated heat spreader between the CM5 and aluminium. Set its height and compression without loading the module's connectors or fouling the controls. Keep the enclosure fanless and verify cooling and touch temperature on an insulated pedalboard as well as on the bench.

Keep the PCM3060 codec and its clocks, instrument buffers, bypass relay and headphone amp away from the power converter and USB wiring. Insulated audio jacks let the electrical design establish the codec ground join and chassis bonds deliberately. The normally closed relay gives each input a direct path to its corresponding line output during bypass, reset and loss of power. The headphone jack has its own amplified path. Verify power margins with startup transients, headphone load and a bus-powered keyboard attached. Keep the barrel and USB-C supplies isolated. Verify USB-C-only operation against the source's advertised current. [USB Type-C current advertisement](https://www.usb.org/sites/default/files/USB%20Type-C%20Spec%20R2.0%20-%20August%202019.pdf)

### Enclosure release checks

- [x] Draw the control and connector layout against the manufacturer's envelope.
- [ ] Freeze the actual parts and verify the 3D stack at tolerance, including full switch travel, all plugged connectors, bosses, ground hardware, cable tails and bottom lid removal.
- [ ] Dry-fit the parts in a mockup. Check boot clearance for both footswitches, hand access to all controls, glare and LED visibility, and readable labels with cables fitted.
- [ ] Verify shell grounding and audio noise with USB, barrel power and two grounded audio devices connected. Check channel-by-channel unpowered bypass, watchdog/restart bypass and separate headphone behaviour.
- [ ] Verify simultaneous USB-C link and input power with USB-A MIDI and both TRS ports active. Exercise source changes and brownout with relay bypass. Check for VBUS back-feed in both supply configurations.
- [ ] Play a source patch without a computer from USB, 5-pin DIN and native Type A keyboards, and a Type B keyboard with the correct adapter. Check note-on, note-off, sustain and controller messages, and startup with a bus-powered keyboard attached.
- [ ] Verify USB-C MIDI playback from a computer, phone and tablet. Exercise simultaneous input on all three transports and verify merged event timing. Check MIDI OUT loopback for control changes from the panel.
- [ ] Run [DEVICE-VERIFY] in the assembled case. Record thermal throttling, touch temperature, MIDI note-to-sound latency and audio dropouts on the 24-hour run.

```mermaid
flowchart TB
  CM[Compute Module 5] <-- I2S --> Codec[PCM3060]
  CM -- SPI --> KADC[MCP3208: eight knobs]
  CM -- GPIO --> SW[Toggles, footswitches, LEDs]
  CM -- UART --> TRS[TRS MIDI in and out]
  CM -- USB --> UC[USB-C device: MIDI + link]
  CM -- USB --> UA[USB-A host]
  CM -- GPIO --> RL[Bypass relay]
  IN[Input jacks] --> BUF[Instrument buffer] --> Codec
  Codec --> OUT[Output jacks]
  Codec --> HP[Headphone amp and jack]
  IN -. bypass .-> RL -. bypass .-> OUT
```

## Device face in Prism [PRISM-INSTRUMENT-DEVICE]

Choosing a profile in the Instrument view's Edit mode draws the panel above at its real proportions with the same Instrument dial used across Prism ([PRISM-INSTRUMENT](../prism-timeline/README.md#instrument-builder-prism-instrument)). Controls cannot be moved or resized. Selecting a control lists the patch's macros that fit its kind; choosing one assigns it, and a macro already on another control moves with a notice. Unassigned controls are drawn dimmed with "Unassigned". A connected device's knob moves turn the on-screen dials live ([DEVICE-LINK-LIVE]).

The Devices panel lists each connected device with its name, firmware, role, active slot and the patch it is playing, with **Open patch**, **Deploy to device…** and **Update firmware…**. Deploy shows the profile check first: every refusal by node and reason, and the patch's cost against the budget as one bar.

Design screenshots to produce, in the style of the [Prism timeline designs](../prism-timeline/README.md): the device face with four assigned knobs and a footswitch, assigning a macro to a three-way switch, the Devices panel with a device playing an unknown patch, and a deploy refused for budget.
