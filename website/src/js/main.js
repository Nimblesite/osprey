import { initThemeToggle } from "/techdoc/js/theme-toggle.js";
import { initLanguageSwitcher } from "/techdoc/js/language-switcher.js";

const toggle = document.getElementById("mobile-menu-toggle");
const links = document.getElementById("site-navigation");
const mobile = window.matchMedia("(max-width: 900px)");
const isMenuOpen = () => toggle?.getAttribute("aria-expanded") === "true";

function setMenu(open, focus = false) {
  toggle.setAttribute("aria-expanded", String(open));
  toggle.setAttribute("aria-label", open ? "Close menu" : "Open menu");
  links.classList.toggle("open", open);
  document.body.classList.toggle("menu-open", open);
  if (focus) (open ? links.querySelector("a") : toggle)?.focus();
}

function initMobileMenu() {
  if (!toggle || !links) return;
  toggle.addEventListener("click", () => setMenu(!isMenuOpen(), !isMenuOpen()));
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && isMenuOpen()) setMenu(false, true);
  });
  document.addEventListener("click", (event) => {
    if (!event.target.closest(".site-header") || event.target.closest(".nav-link")) setMenu(false);
  });
  document.addEventListener("focusin", (event) => {
    if (!event.target.closest(".site-header") && isMenuOpen()) setMenu(false);
  });
  mobile.addEventListener("change", () => setMenu(false));
}

initThemeToggle();
initLanguageSwitcher();
initMobileMenu();
