// Osprey Native website progressive enhancement:
// 1. Accessible tabs (focusgroup="tablist nomemory" + arrow fallback + hidden="until-found" beforematch sync)
// 2. Interactive Accessibility Tree & @osprey/uitest explorer
// 3. Optional Light / Dark / Auto color-scheme toggle

(() => {
  // --- 1. Accessible Tabs ---
  for (const shell of document.querySelectorAll("[data-tabs]")) {
    const tablist = shell.querySelector('[role="tablist"]');
    const tabs = Array.from(shell.querySelectorAll('[role="tab"]'));
    const panels = Array.from(shell.querySelectorAll('[role="tabpanel"]'));

    function activate(selectedTab, { focus = false } = {}) {
      const idx = tabs.indexOf(selectedTab);
      if (idx === -1) return;

      tabs.forEach((tab, i) => {
        const isSelected = i === idx;
        tab.setAttribute("aria-selected", String(isSelected));
        tab.toggleAttribute("focusgroupstart", isSelected);
        tab.tabIndex = isSelected ? 0 : -1;
      });

      panels.forEach((panel, i) => {
        const isSelected = i === idx;
        if (isSelected) {
          panel.removeAttribute("hidden");
          panel.setAttribute("tabindex", "0");
        } else {
          panel.hidden = "until-found";
          panel.removeAttribute("tabindex");
        }
      });

      selectedTab.scrollIntoView({ block: "nearest", inline: "nearest" });
      if (focus) selectedTab.focus();
    }

    // Initialize roving tabindex fallback for browsers without native focusgroup
    const initial = tabs.find((t) => t.getAttribute("aria-selected") === "true") || tabs[0];
    if (initial) activate(initial);

    tabs.forEach((tab) => {
      tab.addEventListener("click", () => activate(tab));
    });

    // Keyboard arrow navigation fallback when native focusgroup is unsupported
    tablist?.addEventListener("keydown", (event) => {
      const current = tabs.indexOf(document.activeElement);
      if (current === -1) return;
      let next = -1;
      if (event.key === "ArrowRight") next = (current + 1) % tabs.length;
      else if (event.key === "ArrowLeft") next = (current - 1 + tabs.length) % tabs.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      if (next !== -1) {
        event.preventDefault();
        activate(tabs[next], { focus: true });
      }
    });

    // Sync tab selection when browser Find-in-Page reveals a hidden="until-found" panel
    panels.forEach((panel, i) => {
      panel.addEventListener("beforematch", () => {
        if (tabs[i]) activate(tabs[i]);
      });
    });
  }

  // --- 2. Interactive Accessibility Tree & @osprey/uitest Explorer ---
  const nodeButtons = Array.from(document.querySelectorAll(".a11y-node-btn"));
  const titleEl = document.getElementById("a11y-inspector-title");
  const roleEl = document.getElementById("a11y-role");
  const boundsEl = document.getElementById("a11y-bounds");
  const valueEl = document.getElementById("a11y-value");
  const bridgeEl = document.getElementById("a11y-bridge");
  const testCodeEl = document.getElementById("a11y-test-code");
  const liveRegion = document.getElementById("a11y-live-region");

  nodeButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      nodeButtons.forEach((other) => other.setAttribute("aria-pressed", String(other === btn)));
      const label = btn.dataset.label || "";
      const role = btn.dataset.role || "";
      if (titleEl) titleEl.textContent = `Inspected Control: ${label}`;
      if (roleEl) roleEl.textContent = role;
      if (boundsEl) boundsEl.textContent = btn.dataset.bounds || "";
      if (valueEl) valueEl.textContent = btn.dataset.value || "";
      if (bridgeEl) bridgeEl.textContent = btn.dataset.accesskit || "";
      if (testCodeEl) testCodeEl.textContent = btn.dataset.test || "";
      if (liveRegion) liveRegion.textContent = `Selected ${label} (${role}) in accessibility tree inspector.`;
    });
  });

  // --- 3. Light / Dark / Auto Theme Toggle ---
  const toggleBtn = document.getElementById("theme-toggle");
  const themeLabel = document.getElementById("theme-label");
  const modes = ["auto", "light", "dark"];
  let modeIndex = 0;

  function applyTheme(mode) {
    if (mode === "auto") {
      document.documentElement.removeAttribute("data-theme");
      toggleBtn?.setAttribute("aria-pressed", "false");
      if (themeLabel) themeLabel.textContent = "Theme: Auto";
    } else {
      document.documentElement.setAttribute("data-theme", mode);
      toggleBtn?.setAttribute("aria-pressed", "true");
      if (themeLabel) themeLabel.textContent = mode === "dark" ? "Theme: Dark" : "Theme: Light";
    }
  }

  toggleBtn?.addEventListener("click", () => {
    modeIndex = (modeIndex + 1) % modes.length;
    applyTheme(modes[modeIndex]);
  });
})();
