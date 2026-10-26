import { readFileSync } from "node:fs";

const source = new URL("../src/_data/navigation.json", import.meta.url);

function navigationLink(item, current, pageUrl) {
  const active = item === current;
  const link = item.link.replace(/class="nav-link[^"]*"/, `class="nav-link${active ? " active" : ""}"`);
  const marked = active ? link.replace("<a ", `<a aria-current="${pageUrl === item.url ? "page" : "location"}" `) : link;
  return `<li>${marked}</li>`;
}

function navigationGroup(items, current, pageUrl) {
  const links = items.map(item => navigationLink(item, current, pageUrl)).join("");
  const label = items[0].group;
  if (!label) return links;
  const active = current?.group === label ? " active" : "";
  return `<li><details class="nav-group" name="site-menu"><summary class="nav-link${active}">${label}</summary><ul class="nav-submenu">${links}</ul></details></li>`;
}

function groupedNavigation(markup, pageUrl) {
  const data = JSON.parse(readFileSync(source, "utf8")).main;
  const links = [...markup.matchAll(/<li>\s*(<a\b[\s\S]*?<\/a>)\s*<\/li>/g)];
  if (links.length !== data.length) throw new Error("Theme navigation does not match navigation.json");
  const items = data.map((item, index) => ({ ...item, link: links[index][1] }));
  const current = items.filter(item => !item.external && pageUrl.startsWith(item.url)).sort((a, b) => b.url.length - a.url.length)[0];
  const groups = [...new Set(items.map(item => item.group || item.text))];
  return groups.map(group => navigationGroup(items.filter(item => (item.group || item.text) === group), current, pageUrl)).join("");
}

export function renderNavigation(content, pageUrl) {
  if (!content.includes('<header class="site-header">')) return content;
  const pattern = /<ul class="nav-links">([\s\S]*?)<\/ul>/;
  if (!pattern.test(content)) throw new Error("Theme header navigation was not found");
  return content.replace(pattern, (_, markup) => `<ul class="nav-links" id="site-navigation">${groupedNavigation(markup, pageUrl)}</ul>`);
}
