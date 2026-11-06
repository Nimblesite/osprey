// Progressive enhancement of static documentation. Implements [DOC-EXPORT-HTML].
(function () {
  const q = document.getElementById('q');
  const out = document.getElementById('results');
  const menu = document.getElementById('menu');
  const narrow = window.matchMedia('(max-width:860px)');
  const data = window.OSPREY_SEARCH;
  const groups = document.querySelectorAll('.nav-group');
  groups.forEach(group => {
    group.open = !!group.querySelector('[aria-current="page"]');
  });
  function fit() { menu.open=!narrow.matches; }
  fit();
  narrow.addEventListener('change', fit);

  const toc = document.querySelector('.toc');
  const headings = document.querySelectorAll('.article h2[id], .article h3[id]');
  if (headings.length) {
    const title = document.createElement('p');
    title.className = 'on-page';
    title.textContent = 'On this page';
    toc.append(title);
    headings.forEach(heading => {
      const link = document.createElement('a');
      link.href = '#' + encodeURIComponent(heading.id);
      link.textContent = heading.textContent;
      link.className = heading.tagName === 'H3' ? 'toc-nested' : '';
      toc.append(link);
    });
    toc.hidden = false;
  }

  document.querySelectorAll('.article > p').forEach(paragraph => {
    const label = paragraph.firstElementChild;
    if (label && label.tagName === 'STRONG' && label.textContent === 'Signature:') {
      paragraph.classList.add('signature');
    }
  });

  function esc(s) {
    return String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
  }
  function show(items, term) {
    menu.hidden = !!term;
    if (!term) { out.innerHTML = ''; return; }
    if (!items.length) {
      out.innerHTML = '<p class="note">No page matches ' + esc(term) + '.</p>';
      return;
    }
    out.innerHTML = '<ul class="hits">' + items.slice(0, 25).map(p =>
      '<li><a href="{root}' + esc(p.slug) + '.html">' + esc(p.title) +
      '</a><p>' + esc(p.summary || p.group) + '</p></li>').join('') + '</ul>';
  }
  function run() {
    const term = q.value.trim(), low = term.toLowerCase();
    const matches = data.filter(p => (p.title + ' ' + p.group + ' ' + p.summary + ' ' + p.body).toLowerCase().includes(low));
    matches.sort((a, b) => Number(!a.title.toLowerCase().includes(low)) - Number(!b.title.toLowerCase().includes(low)));
    show(matches, term);
  }
  if (!data) { out.innerHTML = '<p class="note">Search index unavailable.</p>'; return; }
  q.addEventListener('input', run);
  q.addEventListener('keydown', e => { if (e.key === 'Escape') { q.value = ''; run(); } });
  document.addEventListener('keydown', e => {
    const editing = e.target.matches('input, textarea, select, [contenteditable="true"]');
    if (e.key === '/' && !editing && !e.ctrlKey && !e.metaKey && !e.altKey) { e.preventDefault(); q.focus(); }
  });
  run();
})();
