(() => {
  // 1. Studio 3-View Tabs (Timeline / Track patch / Bus routing)
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

  // 2. Interactive Patch Bay Inspector
  const nodeData = {
    exciter: {
      kicker: "Source · Node 0",
      title: "Exciter 0",
      desc: "Single-period filtered noise impulse that energizes waveguide and modal chains.",
      params: [
        ["Strength", "0.90"],
        ["Follow", "Struck (velocity)"],
        ["Outlet 0", "→ Strike comb 1"],
      ],
    },
    waveguide: {
      kicker: "Physical · Node 2",
      title: "Waveguide String 2",
      desc: "Fractional-delay waveguide string damped cleanly when the note gate closes.",
      params: [
        ["Pitch", "261.63 Hz (C4)"],
        ["Follow", "Pitched (note Hz)"],
        ["Outlet 0", "→ Loss filter 3"],
      ],
    },
    filter: {
      kicker: "Filter · Node 3",
      title: "Modulated Lowpass 3",
      desc: "Audio-rate low-pass filter with a normalized purple modulation inlet for LFOs.",
      params: [
        ["Cutoff", "1,840 Hz"],
        ["Modulation", "LFO → CUTOFF"],
        ["Monitor", "Cyan → Scope"],
      ],
    },
    grain: {
      kicker: "Granular · Node 4",
      title: "Grain Cloud 4",
      desc: "16-voice stereo granular engine with WAV import (8–192 kHz), reverse rate, and jitter.",
      params: [
        ["Pool", "16 overlapping grains"],
        ["Playback rate", "-0.50× (reverse)"],
        ["Outlet", "Stereo → Space / Bus"],
      ],
    },
    bus: {
      kicker: "Group Bus · Stereo",
      title: "Undertow Bus",
      desc: "Named stereo group with gain, 3-band EQ, kick-keyed compressor detector, and cycle guard.",
      params: [
        ["Inputs", "sub, Rivet pulse (6%)"],
        ["Detector", "kick · 4.00× ratio"],
        ["Output", "→ Master"],
      ],
    },
    master: {
      kicker: "Final Stage · Stereo Out",
      title: "Master",
      desc: "The sole playback and export sink. Zero hidden limiters, clippers, or auto-gain.",
      params: [
        ["Sample rate", "48 kHz · 32-bit float"],
        ["Hidden DSP", "None (100% visible)"],
        ["Export", "song.wav (Master only)"],
      ],
    },
  };

  const nodeCards = document.querySelectorAll("[data-node]");
  const insKicker = document.getElementById("inspector-kicker");
  const insTitle = document.getElementById("inspector-title");
  const insDesc = document.getElementById("inspector-desc");
  const insList = document.getElementById("inspector-list");

  const renderInspector = (id) => {
    const info = nodeData[id];
    if (!info || !insKicker || !insTitle || !insDesc || !insList) return;
    insKicker.textContent = info.kicker;
    insTitle.textContent = info.title;
    insDesc.textContent = info.desc;
    insList.replaceChildren(
      ...info.params.map(([k, v]) => {
        const li = document.createElement("li");
        const key = document.createElement("span");
        key.className = "dim";
        key.textContent = k;
        const val = document.createElement("span");
        val.className = "mono";
        val.textContent = v;
        li.append(key, val);
        return li;
      })
    );
  };

  nodeCards.forEach((card) => {
    card.addEventListener("click", () => {
      const id = card.getAttribute("data-node");
      nodeCards.forEach((c) => {
        const active = c === card;
        c.classList.toggle("is-selected", active);
        c.setAttribute("aria-pressed", String(active));
      });
      renderInspector(id);
    });
  });

  // 3. Instrument Category Filter (/modules/)
  const filterButtons = document.querySelectorAll("[data-filter]");
  const filterItems = document.querySelectorAll("[data-category]");
  filterButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      const wanted = btn.getAttribute("data-filter");
      filterButtons.forEach((b) =>
        b.setAttribute("aria-pressed", String(b === btn))
      );
      filterItems.forEach((item) => {
        item.hidden =
          wanted !== "all" && item.getAttribute("data-category") !== wanted;
      });
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

    // Fallback for browsers without <dialog closedby="any"> support (e.g. Safari)
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
