// The one side panel: the documentation navigation, plus the article's own
// h2/h3 headings ("On this page"). The headings nest under the current page's
// link when the page is in the navigation, and open a first section otherwise.
// The desktop panel scrolls on its own: the current page starts in view and
// the scroll-spy keeps the active heading in view without moving the page.
(function () {
  const article = document.querySelector(".prose");
  const panels = Array.from(document.querySelectorAll(".toc-aside .docs-nav"));
  if (!article || !panels.length) return;
  const heads = Array.from(article.querySelectorAll("h2[id], h3[id]"));
  const mobile = document.querySelector(".docs-nav-mobile");
  const desktop = document.querySelector(".docs-nav-desktop");

  const headingList = () => {
    const list = document.createElement("ul");
    list.className = "toc-list";
    for (const h of heads) {
      const li = document.createElement("li");
      li.className = "toc-" + h.tagName.toLowerCase();
      const a = document.createElement("a");
      a.className = "toc-link";
      a.href = "#" + h.id;
      a.textContent = h.textContent.trim();
      li.appendChild(a);
      list.appendChild(li);
    }
    return list;
  };

  const attach = (nav) => {
    const current = nav.querySelector('a[aria-current="page"]');
    if (current) return current.parentElement.appendChild(headingList());
    const section = document.createElement("section");
    section.className = "docs-nav-section docs-nav-current";
    const title = document.createElement("p");
    title.className = "docs-nav-section-title";
    title.textContent = "On this page";
    section.append(title, headingList());
    const first = nav.querySelector(".docs-nav-section");
    nav.insertBefore(section, first);
  };

  // Scroll a link into the middle of the panel without scrolling the page.
  const reveal = (nav, link, always) => {
    if (!nav || !link || nav.scrollHeight <= nav.clientHeight) return;
    const top = link.getBoundingClientRect().top - nav.getBoundingClientRect().top + nav.scrollTop;
    const visible = top >= nav.scrollTop && top + link.offsetHeight <= nav.scrollTop + nav.clientHeight;
    if (always || !visible) nav.scrollTop = top - nav.clientHeight / 3;
  };

  if (heads.length >= 2) panels.forEach(attach);
  reveal(desktop, desktop && desktop.querySelector('a[aria-current="page"]'), true);
  // Choosing a heading on mobile closes the menu so the section is visible.
  if (mobile) mobile.addEventListener("click", (e) => { if (e.target.closest(".toc-link")) mobile.open = false; });

  if (heads.length < 2) return;
  // Scroll-spy: the active heading is the last one above the reading line,
  // which sits just below the page's scroll padding (the sticky header).
  // A heading chosen in the panel stays selected until the reader scrolls,
  // because headings near the end can never reach the reading line.
  const links = Array.from(document.querySelectorAll(".toc-link"));
  let chosen = null;
  let pending = false;
  const spy = () => {
    pending = false;
    const line = parseFloat(getComputedStyle(document.documentElement).scrollPaddingTop) + 24;
    const current = chosen || heads.filter((h) => h.getBoundingClientRect().top <= line).pop() || heads[0];
    for (const l of links) l.classList.toggle("active", l.getAttribute("href") === "#" + current.id);
    reveal(desktop, desktop && desktop.querySelector(".toc-link.active"), false);
  };
  const queue = () => { if (!pending) { pending = true; requestAnimationFrame(spy); } };
  for (const l of links) l.addEventListener("click", () => { chosen = document.getElementById(l.getAttribute("href").slice(1)); queue(); });
  for (const type of ["wheel", "touchmove", "keydown"]) addEventListener(type, () => { chosen = null; }, { passive: true });
  addEventListener("scroll", queue, { passive: true });
  spy();
})();
