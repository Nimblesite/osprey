// Original procedural wireframe flight, rendered offline; never loaded by the site.
const canvas = document.querySelector('canvas');
const context = canvas.getContext('2d');
const tau = Math.PI * 2;
const project = ([x, y, z]) => [720 + x * 0.94 + y * 0.28, 420 + y * 0.8 - x * 0.21 - z];

function strand(points, alpha, width = 1) {
  context.beginPath();
  points.map(project).forEach(([x, y], i) => i ? context.lineTo(x, y) : context.moveTo(x, y));
  context.strokeStyle = `rgba(119, 215, 244, ${alpha})`;
  context.lineWidth = width;
  context.stroke();
}

function wing(side, span, chord, time) {
  const lift = Math.sin(time * tau) * 16 * span * span;
  return [side * (24 + span * 485), -span * 190 + chord * (85 + Math.sin(span * Math.PI) * 95) * (1 - span * 0.7),
    Math.sin(span * Math.PI) * 45 + chord * 32 + lift];
}

function feathers(side, time) {
  for (let i = 0; i <= 42; i++) {
    const span = i / 42;
    const points = Array.from({ length: 32 }, (_, j) => wing(side, span + (1 - span) * j / 130, j / 31, time));
    strand(points, 0.2 + span * 0.35, 1.1);
  }
  for (let i = 0; i <= 18; i++) {
    strand(Array.from({ length: 90 }, (_, j) => wing(side, j / 89, i / 18, time)), 0.15 + i / 55);
  }
  context.shadowColor = '#77d7f4'; context.shadowBlur = 8;
  strand(Array.from({ length: 100 }, (_, j) => wing(side, j / 99, 0, time)), 0.9, 1.8);
  context.shadowBlur = 0;
}

function body() {
  for (let i = 0; i <= 20; i++) {
    const angle = i / 20 * tau;
    const points = Array.from({ length: 65 }, (_, j) => {
      const t = j / 64;
      const radius = Math.sin(t * Math.PI) * (28 - t * 10);
      return [Math.cos(angle) * radius, -80 + t * 235, Math.sin(angle) * radius + 15];
    });
    strand(points, 0.4);
  }
  strand([[0, -80, 15], [0, -114, 8], [12, -86, 12]], 0.9, 1.8);
  for (let i = -8; i <= 8; i++) strand([[0, 110, 10], [i * 7, 220 - Math.abs(i) * 4, 0]], 0.45);
}

function signals(time) {
  for (const side of [-1, 1]) {
    for (let i = 0; i < 12; i++) {
      const span = (time + i / 12) % 1;
      const [x, y] = project(wing(side, span, (i % 4) / 5, time));
      context.fillStyle = `rgba(205, 246, 255, ${Math.sin(span * Math.PI) * 0.9})`;
      context.beginPath(); context.arc(x, y, 1.6, 0, tau); context.fill();
    }
  }
}

function background(time) {
  context.fillStyle = '#0c1325'; context.fillRect(0, 0, 1440, 960);
  const glow = context.createRadialGradient(740, 415, 15, 740, 415, 480);
  glow.addColorStop(0, '#123147'); glow.addColorStop(0.5, '#0d1c31'); glow.addColorStop(1, '#0c1325');
  context.fillStyle = glow; context.fillRect(0, 0, 1440, 960);
  for (let i = 0; i < 125; i++) {
    const x = (i * 173.71) % 1440, y = (i * 211.13) % 960;
    context.fillStyle = `rgba(119,215,244,${0.06 + 0.08 * (1 + Math.sin(time * tau + i))})`;
    context.fillRect(x, y, 1, 1);
  }
}

window.drawFlight = (time) => {
  background(time);
  feathers(-1, time); feathers(1, time); body(); signals(time);
};
window.drawFlight(0);

function animateFlight(duration) {
  const start = performance.now();
  return new Promise((resolve) => {
    const frame = (now) => {
      window.drawFlight(((now - start) % duration) / duration);
      if (now - start < duration) requestAnimationFrame(frame); else resolve();
    };
    requestAnimationFrame(frame);
  });
}

window.recordFlight = async () => {
  const stream = canvas.captureStream(30);
  const recorder = new MediaRecorder(stream, { mimeType: 'video/webm;codecs=vp9', videoBitsPerSecond: 1600000 });
  const chunks = [];
  recorder.ondataavailable = (event) => chunks.push(event.data);
  const stopped = new Promise((resolve) => { recorder.onstop = resolve; });
  recorder.start();
  await animateFlight(8000);
  recorder.stop();
  await stopped;
  stream.getTracks().forEach((track) => track.stop());
  return Array.from(new Uint8Array(await new Blob(chunks).arrayBuffer()));
};
