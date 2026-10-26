import { initThemeToggle } from "/techdoc/js/theme-toggle.js";
import { initLanguageSwitcher } from "/techdoc/js/language-switcher.js";

const toggle = document.getElementById("mobile-menu-toggle");
const links = document.getElementById("site-navigation");
const mobile = window.matchMedia("(max-width: 900px)");
const isMenuOpen = () => toggle?.getAttribute("aria-expanded") === "true";

function closeGroups(except) {
  for (const group of links.querySelectorAll(".nav-group[open]")) {
    if (group !== except) group.open = false;
  }
}

function setMenu(open, focus = false) {
  toggle.setAttribute("aria-expanded", String(open));
  toggle.setAttribute("aria-label", open ? "Close menu" : "Open menu");
  links.classList.toggle("open", open);
  document.body.classList.toggle("menu-open", open);
  if (!open) closeGroups();
  if (focus) (open ? links.querySelector("summary, a") : toggle)?.focus();
}

function dismissMenu(event) {
  if (event.key !== "Escape") return;
  const group = links.querySelector(".nav-group[open]");
  if (group) {
    group.open = false;
    group.querySelector("summary").focus();
  } else if (isMenuOpen()) setMenu(false, true);
}

function initMobileMenu() {
  if (!toggle || !links) return;
  toggle.addEventListener("click", () => setMenu(!isMenuOpen(), !isMenuOpen()));
  document.addEventListener("keydown", dismissMenu);
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".site-header") || event.target.closest("a.nav-link")) setMenu(false);
  });
  document.addEventListener("focusin", (event) => {
    if (!event.target.closest(".site-header")) setMenu(false);
  });
  links.addEventListener("toggle", (event) => { if (event.target.open) closeGroups(event.target); }, true);
  mobile.addEventListener("change", () => setMenu(false));
  document.documentElement.classList.add("nav-ready");
}

initThemeToggle();
initLanguageSwitcher();
initMobileMenu();
