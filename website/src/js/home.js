// The flight is a pre-rendered film: no perpetual canvas or animation loop.
const video = document.querySelector('#flight-video');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
let visible = false;

async function syncMotion() {
  if (reduced.matches || navigator.connection?.saveData || !visible || document.hidden) return video.pause();
  video.muted = true;
  if (!video.getAttribute('src')) video.src = video.dataset.src;
  try {
    await video.play();
  } catch (error) {
    if (error.name === 'AbortError') return;
    video.pause();
  }
}

reduced.addEventListener('change', syncMotion);
navigator.connection?.addEventListener('change', syncMotion);
document.addEventListener('visibilitychange', syncMotion);
new IntersectionObserver(([entry]) => {
  visible = entry.isIntersecting;
  void syncMotion();
}, { threshold: 0.1 }).observe(video);
const flavorControls = document.querySelectorAll('.flavor-controls button[data-flavor]');

function selectFlavor(button) {
  const flavor = button.dataset.flavor;
  for (const control of flavorControls) {
    control.setAttribute('aria-pressed', String(control === button));
  }
  for (const source of document.querySelectorAll('[data-source-flavor]')) {
    source.hidden = source.dataset.sourceFlavor !== flavor;
  }
  document.querySelector('#source-extension').textContent = { default: '.osp', ml: '.ospml', python: '.py' }[flavor];
}

for (const button of flavorControls) {
  button.addEventListener('click', () => selectFlavor(button));
}

const sections = document.querySelectorAll('.home .hero-content, .home .section-heading, .home .language-grid article');
const reveal = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    entry.target.classList.add('is-revealed');
    reveal.unobserve(entry.target);
  }
}, { threshold: 0.08 });
for (const section of sections) reveal.observe(section);
