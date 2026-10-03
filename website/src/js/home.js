// The flight is a pre-rendered film: no perpetual canvas or animation loop.
const video = document.querySelector('#flight-video');
const toggle = document.querySelector('#motion-toggle');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
let enabled = !reduced.matches && !navigator.connection?.saveData;
let visible = false;

function showMotionState() {
  toggle.textContent = enabled ? 'Pause motion' : 'Play motion';
  toggle.setAttribute('aria-label', toggle.textContent);
}

async function syncMotion() {
  showMotionState();
  if (!enabled || !visible || document.hidden) return video.pause();
  if (!video.getAttribute('src')) video.src = video.dataset.src;
  try {
    await video.play();
  } catch (error) {
    if (error.name === 'AbortError') return;
    enabled = false;
    showMotionState();
  }
}

toggle.hidden = false;
toggle.addEventListener('click', () => { enabled = !enabled; void syncMotion(); });
reduced.addEventListener('change', () => { enabled = !reduced.matches; void syncMotion(); });
document.addEventListener('visibilitychange', () => { void syncMotion(); });
new IntersectionObserver(([entry]) => {
  visible = entry.isIntersecting;
  void syncMotion();
}, { threshold: 0.1 }).observe(video);
showMotionState();

function selectFlavor(button) {
  const flavor = button.dataset.flavor;
  for (const control of document.querySelectorAll('[data-flavor]')) {
    control.setAttribute('aria-pressed', String(control === button));
  }
  for (const source of document.querySelectorAll('[data-source-flavor]')) {
    source.hidden = source.dataset.sourceFlavor !== flavor;
  }
  document.querySelector('#source-extension').textContent = flavor === 'ml' ? '.ospml' : '.osp';
}

for (const button of document.querySelectorAll('[data-flavor]')) {
  button.addEventListener('click', () => selectFlavor(button));
}
