const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const repo = path.resolve(__dirname, '../..');
const source = path.join(repo, 'docs/specs');
const destination = path.resolve(__dirname, '../src/spec');
const sections = require('../../docs/specs/sections.json');
const metadataFiles = new Set(['README.md', 'index.md']);
const slugOf = (file) => path.basename(file, '.md').toLowerCase();
const urlOf = (file) => `/spec/${slugOf(file)}/`;

function specFiles(directory, section) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) return specFiles(file, section);
    return entry.name.endsWith('.md') && !metadataFiles.has(entry.name) ? [{ file, section }] : [];
  });
}

function collectSpecs() {
  const allowed = new Set(sections.map((section) => section.slug));
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    if (entry.isDirectory() && !allowed.has(entry.name)) throw new Error(`Unclassified spec folder: ${entry.name}`);
    if (entry.isFile() && entry.name.endsWith('.md') && !metadataFiles.has(entry.name)) {
      throw new Error(`Move ${entry.name} into its specification subject folder`);
    }
  }
  return sections.flatMap((section) => specFiles(path.join(source, section.slug), section));
}

function titleOf(content, file) {
  const heading = content.match(/^# (.+)$/m);
  if (!heading) throw new Error(`Specification has no title: ${file}`);
  return heading[1].replace(/\s*[—-]?\s*\[.*?\]\s*$/, '').trim();
}

function gitDate(file) {
  try {
    return execFileSync('git', ['log', '-1', '--format=%cI', '--', file], { cwd: repo, encoding: 'utf8' }).trim() || null;
  } catch {
    return null;
  }
}

function resolveLink(target, file, pages) {
  if (/^(?:[a-z][\w+.-]*:|\/|#)/i.test(target)) return target;
  const [local, anchor = ''] = target.split('#');
  const absolute = path.resolve(path.dirname(file), local);
  const page = pages.get(absolute);
  if (page) return `${page.permalink}${anchor ? `#${anchor}` : ''}`;
  const category = sections.find((section) => absolute === path.join(source, section.slug, 'README.md'));
  if (category) return `/spec/${category.slug}/${anchor ? `#${anchor}` : ''}`;
  if (absolute === path.join(source, 'README.md')) return '/spec/';
  const relative = path.relative(repo, absolute).split(path.sep).join('/');
  if (relative.startsWith('../')) throw new Error(`Link escapes repository in ${file}: ${target}`);
  return `https://github.com/Nimblesite/osprey/blob/main/${relative}${anchor ? `#${anchor}` : ''}`;
}

// Fenced blocks and inline code spans are copied verbatim; only prose links resolve.
const cleanContent = (content, file, pages) => content.replace(
  /(```[\s\S]*?```|`[^`\n]*`)|\]\(([^)\s]+)\)/g,
  (match, code, target) => code ?? `](${resolveLink(target, file, pages)})`,
).trim();

function frontMatter(title, permalink, description, date = null) {
  const data = { layout: 'page', title, description, permalink, templateEngineOverride: 'md' };
  if (date) data.date = date;
  return `---\n${Object.entries(data).map(([key, value]) => `${key}: ${JSON.stringify(value)}`).join('\n')}\ntags: [specification, reference]\n---\n\n`;
}

function pagesFrom(specs) {
  const seen = new Set();
  return specs.map(({ file, section }) => {
    const slug = slugOf(file);
    if (seen.has(slug)) throw new Error(`Duplicate specification URL: ${slug}`);
    seen.add(slug);
    const content = fs.readFileSync(file, 'utf8');
    return { file, section, slug, content, title: titleOf(content, file), permalink: urlOf(file), date: gitDate(file) };
  }).sort((left, right) => left.slug.localeCompare(right.slug));
}

function writePage(page, pages) {
  const { title, permalink, date, section, file, content, slug } = page;
  const head = frontMatter(title, permalink, `Osprey ${section.title}: ${title}`, date);
  const breadcrumb = `[Specifications](/spec/) / [${section.title}](/spec/${section.slug}/)\n\n`;
  fs.writeFileSync(path.join(destination, `${slug}.md`), head + breadcrumb + cleanContent(content, file, pages) + '\n');
}

function writeSection(section, pages) {
  const members = pages.filter((page) => page.section.slug === section.slug);
  if (!members.length) throw new Error(`Empty specification category: ${section.slug}`);
  const head = frontMatter(section.title, `/spec/${section.slug}/`, section.description);
  const links = members.map((page) => `- [${page.title}](${page.permalink})`).join('\n');
  const kind = section.kind === 'core' ? 'Core language and compiler' : 'Supporting systems';
  const body = `[All specifications](/spec/) / ${kind}\n\n# ${section.title}\n\n${section.description}\n\n${links}\n`;
  fs.writeFileSync(path.join(destination, `${section.slug}.md`), head + body);
}

function sectionLinks(kind) {
  return sections.filter((section) => section.kind === kind).map((section) =>
    `- [${section.title}](/spec/${section.slug}/) — ${section.description}`,
  ).join('\n');
}

function writeIndex() {
  const head = frontMatter('Osprey specifications', '/spec/', 'Language and compiler contracts, separated from platform, tooling and ecosystem contracts.');
  const body = `# Osprey specifications\n\nChoose a subject. Platform applications and development tools have their own contracts; they are not part of the core language specification.\n\n## Core language and compiler\n\n${sectionLinks('core')}\n\n## Supporting systems\n\n${sectionLinks('support')}\n\n## Contract and delivery\n\nThese documents define required behavior. A specification is not evidence that a feature has shipped. Each contract records its limits; [feature status](/status/) and the linked delivery plans distinguish implemented behavior from unfinished work.\n`;
  fs.writeFileSync(path.join(destination, 'index.md'), head + body);
}

function generate() {
  const pages = pagesFrom(collectSpecs());
  const lookup = new Map(pages.map((page) => [page.file, page]));
  fs.mkdirSync(destination, { recursive: true });
  for (const file of fs.readdirSync(destination)) {
    if (file.endsWith('.md')) fs.unlinkSync(path.join(destination, file));
  }
  pages.forEach((page) => writePage(page, lookup));
  sections.forEach((section) => writeSection(section, pages));
  writeIndex();
  console.log(`Published ${pages.length} contracts in ${sections.length} separate specification subjects.`);
}

module.exports = { cleanContent, titleOf, resolveLink, pagesFrom };
if (require.main === module) {
  try { generate(); } catch (error) { console.error(error.message); process.exit(1); }
}
