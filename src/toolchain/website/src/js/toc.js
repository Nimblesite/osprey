// Builds the prose Table of Contents from the article's own h2/h3 headings
// (which already carry ids from markdown-it-anchor). Pure progressive
// enhancement: no headings → the sidebar is removed and the article goes
// full-width. Adds scroll-spy to highlight the active section.
(function () {
  const article = document.querySelector(".prose");
  const toc = document.getElementById("toc");
  const layout = document.querySelector(".prose-layout");
  if (!article || !toc || !layout) return;

  const heads = Array.from(article.querySelectorAll("h2[id], h3[id]"));
  if (heads.length < 2) {
    layout.classList.add("toc-empty");
    const aside = toc.closest(".toc-aside");
    if (aside) aside.remove();
    return;
  }

  const list = document.createElement("ul");
  list.className = "toc-list";
  const linkById = new Map();
  for (const h of heads) {
    const li = document.createElement("li");
    li.className = "toc-" + h.tagName.toLowerCase();
    const a = document.createElement("a");
    a.className = "toc-link";
    a.href = "#" + h.id;
    a.textContent = h.textContent.trim();
    li.appendChild(a);
    list.appendChild(li);
    linkById.set(h.id, a);
  }
  toc.appendChild(list);

  if (!("IntersectionObserver" in window)) return;
  const observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        linkById.forEach((l) => l.classList.remove("active"));
        const active = linkById.get(e.target.id);
        if (active) active.classList.add("active");
      }
    },
    { rootMargin: "-80px 0px -70% 0px", threshold: 0 }
  );
  heads.forEach((h) => observer.observe(h));
})();
