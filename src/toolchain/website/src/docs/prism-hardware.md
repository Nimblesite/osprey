---
layout: page
title: "Prism hardware"
description: "The open hardware plan for building a standalone Prism synth or effects pedal with your own parts."
permalink: /docs/audio-synthesis/hardware/
---

**Prism hardware is planned and is not in production.**

Build your own standalone synth or FX pedal. The open hardware plan pairs a Raspberry Pi Compute Module 5 with a custom carrier, a metal stompbox enclosure and off-the-shelf parts you source yourself. Plug a USB MIDI keyboard into the single USB-C port and power the pedal from the barrel jack. Use a 3.5 mm TRS MIDI Type A adapter for a 5-pin DIN keyboard.

<figure>
  <img src="/assets/images/prism/device-front.webp" width="1536" height="1024" alt="Front of the charcoal Prism pedal concept with eight knobs, four toggles and two footswitches" decoding="async">
  <img src="/assets/images/prism/device-concept.webp" width="1536" height="1024" alt="Rear of the Prism pedal concept with one USB-C port, DC power and two Type A TRS MIDI jacks" loading="lazy" decoding="async">
  <figcaption>AI-generated concept renders of the front and rear.</figcaption>
</figure>

Design the instrument in Prism's visual patch editor. Connect oscillators, filters and effects in any valid routing, with no fixed signal chain. Assign the controls you want to the physical knobs and switches, then send the patch to the pedal. The editor leaves routing open and the planned device checks each patch against its real-time CPU budget.

The open [Osprey synthesis engine](/docs/audio-synthesis/#fx-on-effects) uses mathematical proofs checked against its production DSP code, including gain curves and antialiasing.

<figure>
  <a href="/assets/images/prism/enclosure.svg"><img src="/assets/images/prism/enclosure.svg" width="1300" height="980" alt="Dimensioned control and port layout with a section through the module mounting bracket" loading="lazy" decoding="async"></a>
  <figcaption>AI-generated layout drawing based on the enclosure design.</figcaption>
</figure>

[Read the hardware specification](/spec/prismdevicehardware/)
