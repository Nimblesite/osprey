// [WORKBENCH-BOARD] Host drawing and hit testing only. Osprey owns strokes,
// framing, undo, clear, recognition and every proof decision.
export const BOARD_SCRIPT = String.raw`
  let boardDrawing = null;
  let deferredBoardRender = null;
  const pendingBoards = new Map();

  function canvasContext(canvas) {
    const context = canvas.getContext("2d");
    if (!context) throw new Error("A drawing board requires a canvas context");
    return context;
  }

  function chalkTexture(colour) {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 32;
    const context = canvasContext(canvas);
    context.fillStyle = colour;
    context.fillRect(0, 0, 32, 32);
    const pixels = context.getImageData(0, 0, 32, 32);
    for (let at = 3; at < pixels.data.length; at += 4) {
      pixels.data[at] = 85 + ((Math.imul(at, 1103515245) >>> 10) % 171);
    }
    context.putImageData(pixels, 0, 0);
    return canvas;
  }

  function chalkStroke(context, stroke, scene) {
    const pattern = context.createPattern(scene.texture, "repeat");
    if (!pattern) throw new Error("The drawing board cannot create its chalk texture");
    context.strokeStyle = pattern;
    context.fillStyle = pattern;
    context.lineWidth = stroke.width;
    context.lineCap = context.lineJoin = "round";
    context.beginPath();
    stroke.points.forEach(function (point, index) {
      if (index === 0) context.moveTo(point[0], point[1]);
      else context.lineTo(point[0], point[1]);
    });
    context.stroke();
    const first = stroke.points[0];
    context.beginPath();
    context.arc(first[0], first[1], stroke.width / 2, 0, Math.PI * 2);
    context.fill();
  }

  function boardTransform(context, scene) {
    const frame = scene.frame;
    context.setTransform(scene.ratio * frame.scale, 0, 0, scene.ratio * frame.scale, scene.ratio * frame.x, scene.ratio * frame.y);
  }

  function boardFrame(canvas, state) {
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    const scale = Math.min(width / state.size.width, height / state.size.height) * state.view.scale / 100;
    return { scale: scale, x: (width - state.size.width * scale) / 2 + state.view.x * scale,
      y: (height - state.size.height * scale) / 2 + state.view.y * scale };
  }

  function paintPending(scene) {
    const context = canvasContext(scene.canvas);
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, scene.canvas.width, scene.canvas.height);
    context.drawImage(scene.base, 0, 0);
    boardTransform(context, scene);
    const pending = pendingBoards.get(scene.canvas.dataset.boardSlice) || [];
    pending.forEach(function (entry) { chalkStroke(context, entry.stroke, scene); });
    if (boardDrawing && boardDrawing.scene === scene) {
      if (boardDrawing.lasso) paintLasso(context, boardDrawing.points, scene);
      else chalkStroke(context, { points: boardDrawing.points, width: 3 }, scene);
    }
  }

  function paintLasso(context, points, scene) {
    context.strokeStyle = scene.colour;
    context.lineWidth = 1 / scene.frame.scale;
    context.setLineDash([6 / scene.frame.scale, 5 / scene.frame.scale]);
    context.beginPath();
    points.forEach(function (point, index) {
      if (index === 0) context.moveTo(point[0], point[1]);
      else context.lineTo(point[0], point[1]);
    });
    context.closePath();
    context.stroke();
    context.setLineDash([]);
  }

  function resizeBoard(scene) {
    scene.ratio = window.devicePixelRatio || 1;
    scene.canvas.width = scene.base.width = Math.max(1, Math.round(scene.canvas.clientWidth * scene.ratio));
    scene.canvas.height = scene.base.height = Math.max(1, Math.round(scene.canvas.clientHeight * scene.ratio));
    scene.frame = boardFrame(scene.canvas, scene.state);
    const context = canvasContext(scene.base);
    boardTransform(context, scene);
    scene.state.strokes.forEach(function (stroke) { chalkStroke(context, stroke, scene); });
    paintPending(scene);
  }

  function boardPoint(event, scene) {
    const box = scene.canvas.getBoundingClientRect();
    return [Math.round(Math.max(0, Math.min(scene.state.size.width, (event.clientX - box.left - scene.frame.x) / scene.frame.scale))),
      Math.round(Math.max(0, Math.min(scene.state.size.height, (event.clientY - box.top - scene.frame.y) / scene.frame.scale)))];
  }

  function startBoard(event, scene) {
    if (!event.isPrimary || event.button !== 0 || boardDrawing) return;
    event.preventDefault();
    scene.canvas.focus();
    boardDrawing = { scene: scene, pointer: event.pointerId, points: [boardPoint(event, scene)],
      lasso: event.shiftKey && scene.canvas.dataset.lassoEnabled === "true", control: controlOf(scene.canvas) };
    if (!boardDrawing.control) throw new Error("The drawing board has no issued provenance");
    scene.canvas.setPointerCapture(event.pointerId);
    paintPending(scene);
  }

  function moveBoard(event, scene) {
    if (!boardDrawing || boardDrawing.scene !== scene || boardDrawing.pointer !== event.pointerId) return;
    const point = boardPoint(event, scene);
    const points = boardDrawing.points;
    const last = points[points.length - 1];
    if (point[0] === last[0] && point[1] === last[1]) return;
    points.push(point);
    if (points.length >= 8192) boardDrawing.points = points.filter(function (_, index) { return index % 2 === 0 || index === points.length - 1; });
    requestAnimationFrame(function () { paintPending(scene); });
  }

  function insideBoardPolygon(point, polygon) {
    let inside = false;
    for (let first = 0, previous = polygon.length - 1; first < polygon.length; previous = first++) {
      const a = polygon[first], b = polygon[previous];
      if ((a[1] > point[1]) !== (b[1] > point[1]) && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]) inside = !inside;
    }
    return inside;
  }

  function selectedBoardStrokes(drawing) {
    return drawing.scene.state.strokes.reduce(function (indices, stroke, index) {
      if (stroke.points.some(function (point) { return insideBoardPolygon(point, drawing.points); })) indices.push(index);
      return indices;
    }, []);
  }

  function sendBoardStroke(drawing) {
    const stroke = { points: drawing.points, width: 3, at: Date.now() };
    const id = ++editSequence;
    const slice = drawing.scene.canvas.dataset.boardSlice;
    const pending = pendingBoards.get(slice) || [];
    pendingBoards.set(slice, pending.concat([{ id: id, stroke: stroke }]));
    vscode.postMessage(Object.assign({}, drawing.control, { type: "event", edit: id, value: stroke }));
  }

  function finishBoard(event, scene, cancelled) {
    if (!boardDrawing || boardDrawing.scene !== scene || boardDrawing.pointer !== event.pointerId) return;
    const drawing = boardDrawing;
    boardDrawing = null;
    if (!cancelled && drawing.lasso) vscode.postMessage(Object.assign({}, drawing.control,
      { type: "event", ordinal: Number(scene.canvas.dataset.lassoOrdinal), value: selectedBoardStrokes(drawing) }));
    if (!cancelled && !drawing.lasso) sendBoardStroke(drawing);
    paintPending(scene);
    if (deferredBoardRender) {
      const message = deferredBoardRender;
      deferredBoardRender = null;
      renderMessage(message);
    }
  }

  function acknowledgeBoardEdit(message) {
    let found = false;
    pendingBoards.forEach(function (entries, slice) {
      if (!entries.some(function (entry) { return entry.id === message.edit; })) return;
      found = true;
      if (message.accepted === true) pendingBoards.set(slice, entries.filter(function (entry) { return entry.id !== message.edit; }));
      else { pendingLoss = message.error || "A stroke was refused; it remains visible as pending working."; renderDiagnostics(hostDiagnostics); }
    });
    return found;
  }

  function refreshBoards() {
    document.querySelectorAll("canvas[data-board-slice]").forEach(function (canvas) {
      const colour = getComputedStyle(canvas).color;
      const scene = { canvas: canvas, base: document.createElement("canvas"), state: JSON.parse(canvas.dataset.boardState),
        texture: chalkTexture(colour), colour: colour, ratio: 1, frame: null };
      resizeBoard(scene);
      const observer = new ResizeObserver(function () { if (canvas.isConnected) resizeBoard(scene); else observer.disconnect(); });
      observer.observe(canvas);
      canvas.addEventListener("pointerdown", function (event) { startBoard(event, scene); });
      canvas.addEventListener("pointermove", function (event) { moveBoard(event, scene); });
      canvas.addEventListener("pointerup", function (event) { finishBoard(event, scene, false); });
      canvas.addEventListener("pointercancel", function (event) { finishBoard(event, scene, true); });
    });
  }
`;
