(() => {
  // 0. Single-Nav Dropdown Submenu (Learn)
  document.querySelectorAll("[data-dropdown]").forEach((group) => {
    const trigger = group.querySelector("[data-dropdown-trigger]");
    const menu = group.querySelector("[data-dropdown-menu]");
    if (!trigger || !menu) return;
    const setOpen = (open) => {
      trigger.setAttribute("aria-expanded", String(open));
      menu.hidden = !open;
    };
    trigger.addEventListener("click", (event) => {
      event.stopPropagation();
      setOpen(menu.hidden);
    });
    group.addEventListener("mouseenter", () => setOpen(true));
    group.addEventListener("mouseleave", () => setOpen(false));
    document.addEventListener("click", (event) => {
      if (!group.contains(event.target)) setOpen(false);
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setOpen(false);
    });
  });

  // 1. Studio 3-View Screenshot Tabs (Track patch / Timeline / Bus routing)
  const tabButtons = document.querySelectorAll("[data-view-tab]");
  const tabPanels = document.querySelectorAll("[data-view-panel]");
  tabButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      const target = btn.getAttribute("data-view-tab");
      tabButtons.forEach((b) =>
        b.setAttribute("aria-selected", String(b === btn))
      );
      tabPanels.forEach((p) => {
        p.hidden = p.getAttribute("data-view-panel") !== target;
      });
    });
  });

  // 2. Instrument Category Filter (/modules/)
  const filterButtons = document.querySelectorAll("[data-filter]");
  const filterItems = document.querySelectorAll("[data-category]");
  const filterStatus = document.getElementById("filter-status");
  filterButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      const wanted = btn.getAttribute("data-filter");
      filterButtons.forEach((b) =>
        b.setAttribute("aria-pressed", String(b === btn))
      );
      let shown = 0;
      filterItems.forEach((item) => {
        const hide =
          wanted !== "all" && item.getAttribute("data-category") !== wanted;
        item.hidden = hide;
        if (!hide) shown += 1;
      });
      if (filterStatus) {
        filterStatus.textContent =
          wanted === "all"
            ? `Showing all ${shown} instruments`
            : `Showing ${shown} ${wanted} instrument${shown === 1 ? "" : "s"}`;
      }
    });
  });

  // 3. Interactive DSP Module Chip Inspector (/modules/)
  const moduleSpecs = {
    grainCloud: ["Stereo · 16 grains", "Granular engine with 8–192 kHz WAV import, reverse/freeze playback rate, pan, and timing jitter."],
    exciter: ["Mono · Audio", "Single-period filtered noise burst for plucking waveguides and striking modal banks."],
    sine: ["Mono · Audio", "Pure phase-accurate sine oscillator with optional MIDI note inlet."],
    wavetable: ["Mono · Audio", "Interpolated wavetable oscillator with proved linear endpoints and harmonic morphing."],
    additive: ["Mono · Audio", "Harmonic bank synthesizer with proved additive period invariance."],
    operator: ["Mono · Audio", "Phase-distortion FM operator with feedback and bloom control."],
    bowed: ["Mono · Physical", "Continuous stick-slip bowed string model with gated bow pressure and vibrato."],
    midiOscillator: ["Events → Audio", "Note-driven oscillator responding directly to MIDI pitch and velocity streams."],
    waveguide: ["Mono · Physical", "Traveling-wave string resonator backed by Lean proof travelingWavePropagation."],
    modal: ["Mono · Physical", "8-mode coupled resonator bank for bells, bars, and metallic percussion."],
    resonator: ["Mono · Physical", "Tunable resonant body filter for acoustic coloration."],
    strikeComb: ["Mono · Physical", "Comb delay line tuned to string pitch for struck-wire transients."],
    fractionalDelay: ["Mono · DSP", "Sub-sample interpolated delay line for precision pitch waveguides."],
    lossFilter: ["Mono · DSP", "Frequency-dependent damping inside physical feedback loops."],
    allpass: ["Mono · DSP", "Unit-magnitude phase disperser backed by Lean proof allpassMagnitude."],
    shifter: ["Mono · Spectral", "Formant-preserving FFT pitch shifter with wet/dry harmonic blending."],
    spectral: ["Mono · Spectral", "Two-source spectral morphing and phase-locked spectral freeze."],
    modulatedLowpass: ["Audio + Mod → Audio", "Resonant low-pass filter with normalized purple CUTOFF modulation inlet and cyan Scope monitor."],
    lowpass: ["Mono · Filter", "Clean fixed-cutoff low-pass filter."],
    shaper: ["Mono · Nonlinear", "Tanh, hard clip, cubic, and wavefolder shaper with proved antiderivative anti-aliasing."],
    reverb: ["Stereo · Space", "Lush stereo algorithmic reverb for Space sends and ambient tails."],
    delay: ["Mono/Stereo · Time", "Feedback delay line for rhythmic echoes and stereo Haas spreads."],
    smoother: ["Control · DSP", "Zipper-free parameter smoother for live dial and macro sweeps."],
    gain: ["Mono · Utility", "Visible linear gain stage — zero hidden auto-gain."],
    modulatedGain: ["Audio + Mod → Audio", "VCA controlled by LFOs or envelopes for tremolo and AM."],
    stereoModulatedGain: ["Stereo + Mod → Stereo", "True-stereo modulated VCA for granular clouds and wide buses."],
    lfo: ["Control · Mod", "Normalized low-frequency oscillator driving filter cutoffs, gain, and pan."],
    formulaNotes: ["Events · Generative", "Type a step expression like 60 + t * 7 % 12 to emit deterministic MIDI melodies."],
    midiNote: ["Events · MIDI", "Fixed or triggered MIDI note generator for self-playing modular patches."],
    mixer: ["Multi → Mono", "Sum arbitrary audio signals with explicit per-channel levels."],
    stereo: ["Mono → Stereo", "Promote mono signals into stereo space with pan control."],
    downmix: ["Stereo → Mono", "Sum stereo buses cleanly to mono for sidechain or mono FX."],
    split: ["Stereo → 2×Mono", "Split left and right channels for independent per-channel processing."],
    join: ["2×Mono → Stereo", "Recombine dual-mono chains into a stereo pair."],
    route: ["N-in → N-out", "Arbitrary channel matrix and L/R swapper (e.g. channel0 / channel1)."],
    output: ["Mono · Sink", "Mono track patch output."],
    stereoOutput: ["Stereo · Sink", "Stereo track patch output feeding your bus tree or VST3 host."],
  };

  const chipButtons = document.querySelectorAll("[data-module]");
  const modTitle = document.getElementById("module-detail-title");
  const modRate = document.getElementById("module-detail-rate");
  const modDesc = document.getElementById("module-detail-desc");

  chipButtons.forEach((chip) => {
    chip.addEventListener("click", () => {
      const name = chip.getAttribute("data-module");
      const spec = moduleSpecs[name];
      if (!spec || !modTitle || !modRate || !modDesc) return;
      chipButtons.forEach((c) => {
        const active = c === chip;
        c.classList.toggle("is-active", active);
        c.setAttribute("aria-pressed", String(active));
      });
      modTitle.textContent = name;
      modRate.textContent = spec[0];
      modDesc.textContent = spec[1];
    });
  });

  // 4. Native Modal Lightbox with Safari closedBy fallback
  const dialog = document.getElementById("lightbox");
  const lbImg = document.getElementById("lightbox-img");
  const lbTitle = document.getElementById("lightbox-title");
  const lbCaption = document.getElementById("lightbox-caption");

  if (dialog && lbImg && lbTitle && lbCaption) {
    document.querySelectorAll("[data-lightbox-src]").forEach((trigger) => {
      trigger.addEventListener("click", () => {
        lbImg.src = trigger.getAttribute("data-lightbox-src") || "";
        lbImg.alt = trigger.getAttribute("data-lightbox-alt") || "";
        lbTitle.textContent = trigger.getAttribute("data-lightbox-title") || "Studio view";
        lbCaption.textContent = trigger.getAttribute("data-lightbox-caption") || "";
        dialog.showModal();
      });
    });

    if (!("closedBy" in HTMLDialogElement.prototype)) {
      dialog.addEventListener("click", (event) => {
        if (event.target !== dialog) return;
        const rect = dialog.getBoundingClientRect();
        const inside =
          rect.top <= event.clientY &&
          event.clientY <= rect.top + rect.height &&
          rect.left <= event.clientX &&
          event.clientX <= rect.left + rect.width;
        if (!inside) dialog.close();
      });
    }
  }
})();
