(() => {
  // 0. Single-Nav Dropdown Submenu (Learn)
  const finePointer = window.matchMedia("(hover: hover) and (pointer: fine)");
  document.querySelectorAll("[data-dropdown]").forEach((group) => {
    const trigger = group.querySelector("[data-dropdown-trigger]");
    const menu = group.querySelector("[data-dropdown-menu]");
    if (!trigger || !menu) return;
    let openedByHover = false;
    const setOpen = (open) => {
      trigger.setAttribute("aria-expanded", String(open));
      menu.hidden = !open;
      if (!open) openedByHover = false;
    };
    trigger.addEventListener("click", (event) => {
      event.stopPropagation();
      if (openedByHover) {
        openedByHover = false;
        return;
      }
      setOpen(menu.hidden);
    });
    group.addEventListener("mouseenter", () => {
      if (!finePointer.matches) return;
      if (menu.hidden) {
        setOpen(true);
        openedByHover = true;
      }
    });
    group.addEventListener("mouseleave", () => {
      if (finePointer.matches) setOpen(false);
    });
    document.addEventListener("click", (event) => {
      if (!group.contains(event.target)) setOpen(false);
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") setOpen(false);
    });
  });

  // 1. Studio 3-View Screenshot Tabs
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

  // 2. Module Category Filter (/learn/modules/)
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
            ? `Showing all ${shown} modules`
            : `Showing ${shown} ${wanted} module${shown === 1 ? "" : "s"}`;
      }
    });
  });

  // Shared Safari closedBy light-dismiss fallback
  const wireLightDismiss = (dialog) => {
    if (!dialog || "closedBy" in HTMLDialogElement.prototype) return;
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
  };

  // 3. Module Detail Popup Modal (/learn/modules/)
  const modDialog = document.getElementById("module-dialog");
  if (modDialog) {
    const title = document.getElementById("mod-modal-title");
    const cat = document.getElementById("mod-modal-cat");
    const details = document.getElementById("mod-modal-details");
    const ports = document.getElementById("mod-modal-ports");
    const controls = document.getElementById("mod-modal-controls");
    document.querySelectorAll("[data-module-card]").forEach((card) => {
      card.addEventListener("click", () => {
        title.textContent = card.getAttribute("data-mod-name") || "";
        cat.textContent = card.getAttribute("data-mod-cat") || "";
        details.textContent = card.getAttribute("data-mod-details") || "";
        ports.textContent = card.getAttribute("data-mod-ports") || "";
        controls.textContent = card.getAttribute("data-mod-controls") || "";
        modDialog.showModal();
      });
    });
    wireLightDismiss(modDialog);
  }

  // 4. Screenshot Lightbox Modal
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
    wireLightDismiss(dialog);
  }
})();
