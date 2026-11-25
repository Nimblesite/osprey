# Instruments and devices

Prism instruments, their macro controls, and the hardware synths and effects pedals built from them.

[All specification subjects](../README.md)

An **instrument** is a Prism patch with a face: a few named controls, each a macro that moves several patch parameters at once. A **device** is a box that runs that patch without Prism: the engine from `@osprey/synth` on a small computer, with physical knobs and switches in place of the on-screen controls. Prism is the factory. You design the sound and the face on the computer, then deploy them to any device whose profile fits.

- [Instrument builder [PRISM-INSTRUMENT]](PrismInstrument.md): faces, macros, MIDI learn and OSC, and the device faces that follow a hardware profile.
- [Prism device [DEVICE]](PrismDevice.md): the reference hardware, its firmware, the engine it runs, the link to Prism and the factory workflow.

Delivery is tracked in the [Prism hardware plan](../../plans/prism-hardware.md). Design screenshots and typed models are in [Prism timeline designs](../../designs/prism-timeline/README.md#instrument-builder-prism-instrument) and [Prism device designs](../../designs/prism-device/README.md).
