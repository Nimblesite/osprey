import { readFileSync } from "node:fs";

const source = new URL("../src/_data/navigation.json", import.meta.url);

function navigationLink(item, current, pageUrl) {
  const active = item === current;
  const link = item.link.replace(/class="nav-link[^"]*"/, `class="nav-link${active ? " active" : ""}"`);
  const marked = active ? link.replace("<a ", `<a aria-current="${pageUrl === item.url ? "page" : "location"}" `) : link;
  return `<li>${marked}</li>`;
}

const longestPrefix = (entries, pageUrl) =>
  entries.filter(entry => pageUrl.startsWith(entry.url)).sort((a, b) => b.url.length - a.url.length)[0];

// A page marks its own top-level link (Playground, Blog); a page only the docs sidebar lists marks Docs.
function currentItem(items, docs, pageUrl) {
  const docsItem = items.find(item => item.url === "/docs/");
  const own = items.filter(item => !item.external && item !== docsItem);
  const listed = docs.flatMap(section => section.items.map(item => ({ url: item.url, owner: docsItem })));
  const best = longestPrefix([...own.map(item => ({ url: item.url, owner: item })), ...listed], pageUrl);
  return best?.owner;
}

// The menu's links, and the call-to-action link the header shows beside its buttons.
function siteNavigation(markup, pageUrl) {
  const { main: data, docs } = JSON.parse(readFileSync(source, "utf8"));
  const links = [...markup.matchAll(/<li>\s*(<a\b[\s\S]*?<\/a>)\s*<\/li>/g)];
  if (links.length !== data.length) throw new Error("Theme navigation does not match navigation.json");
  const items = data.map((item, index) => ({ ...item, link: links[index][1] }));
  const current = currentItem(items, docs, pageUrl);
  const menu = items.filter(item => !item.cta).map(item => navigationLink(item, current, pageUrl)).join("");
  const cta = items.filter(item => item.cta).map(item => item.link.replace(/class="nav-link[^"]*"/, 'class="nav-cta"')).join("");
  return { menu, cta };
}

export function renderNavigation(content, pageUrl) {
  if (!content.includes('<header class="site-header">')) return content;
  const pattern = /<ul class="nav-links">([\s\S]*?)<\/ul>/;
  if (!pattern.test(content)) throw new Error("Theme header navigation was not found");
  const { menu, cta } = siteNavigation(content.match(pattern)[1], pageUrl);
  return content.replace(pattern, () => `<ul class="nav-links" id="site-navigation">${menu}</ul>`)
    .replace('<div class="nav-actions">', () => `<div class="nav-actions">${cta}`);
}
