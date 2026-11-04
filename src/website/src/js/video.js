// Key moments for a tutorial video. `?t=<seconds>` starts the video there (the
// deep link Google's key moments use), and a chapter or transcript link seeks
// the player in place instead of reloading the page.
(() => {
  const player = document.querySelector("figure.video video");
  if (!player) return;
  const seek = (at, play) => {
    const go = () => {
      player.currentTime = at;
      if (play) player.play().catch(() => {});
    };
    if (player.readyState >= 1) go();
    else player.addEventListener("loadedmetadata", go, { once: true });
  };
  const start = Number(new URLSearchParams(location.search).get("t"));
  if (start > 0) seek(start, false);
  document.addEventListener("click", (event) => {
    const link = event.target.closest("a[data-seek]");
    if (!link) return;
    event.preventDefault();
    const at = Number(link.dataset.seek);
    history.replaceState(null, "", at ? `?t=${at}` : location.pathname);
    player.scrollIntoView({ behavior: "smooth", block: "center" });
    seek(at, true);
  });
})();
