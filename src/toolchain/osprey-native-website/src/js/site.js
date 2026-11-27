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

    function activate(selectedTab, { focus = false, scroll = true } = {}) {
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

      if (scroll) selectedTab.scrollIntoView({ block: "nearest", inline: "nearest" });
      if (focus) selectedTab.focus();
    }

    // Initialize roving tabindex fallback for browsers without native focusgroup
    const initial = tabs.find((t) => t.getAttribute("aria-selected") === "true") || tabs[0];
    if (initial) activate(initial, { scroll: false });

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

  // --- 2. Light / Dark / Auto Theme Toggle ---
  const THEME_STORAGE_KEY = "osprey-native-theme";
  const toggleBtn = document.getElementById("theme-toggle");
  const themeLabel = document.getElementById("theme-label");
  const modes = ["auto", "light", "dark"];

  function readStoredTheme() {
    try {
      const saved = localStorage.getItem(THEME_STORAGE_KEY);
      return saved === "light" || saved === "dark" ? saved : "auto";
    } catch {
      return "auto";
    }
  }

  function writeStoredTheme(mode) {
    try {
      if (mode === "auto") localStorage.removeItem(THEME_STORAGE_KEY);
      else localStorage.setItem(THEME_STORAGE_KEY, mode);
    } catch {
      // Ignore storage errors in restricted browsing contexts.
    }
  }

  let modeIndex = modes.indexOf(readStoredTheme());

  function applyTheme(mode, { persist = true } = {}) {
    if (mode === "auto") {
      document.documentElement.removeAttribute("data-theme");
      toggleBtn?.setAttribute("aria-pressed", "false");
      if (themeLabel) themeLabel.textContent = "Theme: Auto";
    } else {
      document.documentElement.setAttribute("data-theme", mode);
      toggleBtn?.setAttribute("aria-pressed", "true");
      if (themeLabel) themeLabel.textContent = mode === "dark" ? "Theme: Dark" : "Theme: Light";
    }
    if (persist) writeStoredTheme(mode);
  }

  applyTheme(modes[modeIndex], { persist: false });

  toggleBtn?.addEventListener("click", () => {
    modeIndex = (modeIndex + 1) % modes.length;
    applyTheme(modes[modeIndex]);
  });

  window.addEventListener("storage", (event) => {
    if (event.key !== THEME_STORAGE_KEY) return;
    const next = event.newValue === "light" || event.newValue === "dark" ? event.newValue : "auto";
    modeIndex = modes.indexOf(next);
    applyTheme(next, { persist: false });
  });

  // --- 3. Responsive Mobile Menus (Primary Nav & Docs Submenu) ---
  for (const btn of document.querySelectorAll("[data-menu-toggle]")) {
    const target = document.getElementById(btn.getAttribute("data-menu-toggle"));
    if (!target) continue;

    function setOpen(open) {
      btn.setAttribute("aria-expanded", String(open));
      if (open) target.setAttribute("data-open", "true");
      else target.removeAttribute("data-open");
    }

    btn.addEventListener("click", () => setOpen(btn.getAttribute("aria-expanded") !== "true"));
    target.addEventListener("click", (event) => {
      if (event.target.closest("a")) setOpen(false);
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && btn.getAttribute("aria-expanded") === "true") {
        setOpen(false);
        btn.focus();
      }
    });
  }
})();
