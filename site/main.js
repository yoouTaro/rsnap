// Copy buttons: every .cmd block copies its <code> text.
for (const btn of document.querySelectorAll("[data-copy]")) {
  btn.addEventListener("click", async () => {
    const text = btn.parentElement.querySelector("[data-cmd]").textContent;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      return;
    }
    btn.classList.add("done");
    clearTimeout(btn.timer);
    btn.timer = setTimeout(() => btn.classList.remove("done"), 1500);
  });
}

// Draw rsnap's arrow on the hero image: drag from where to where. The
// arrow matches the app's (round shaft, filled head, soft drop shadow).

const box = document.querySelector("[data-draw]");
const canvas = document.querySelector("[data-canvas]");
const clear = document.querySelector("[data-clear]");
const hint = document.querySelector("[data-hint]");
const ctx = canvas.getContext("2d");
const arrows = [];
let live = null;

function fit() {
  const r = box.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(r.width * dpr);
  canvas.height = Math.round(r.height * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  redraw();
}

function arrow(a, b, w) {
  const dx = b.x - a.x, dy = b.y - a.y;
  const len = Math.hypot(dx, dy);
  if (len < 4) return;
  const ux = dx / len, uy = dy / len;
  const head = Math.min(w * 4.6, len * 0.6), half = head * 0.58;
  const bx = b.x - ux * head, by = b.y - uy * head;
  for (const [color, oy] of [["rgba(0,0,0,0.25)", w * 0.45], ["#FF453A", 0]]) {
    ctx.strokeStyle = ctx.fillStyle = color;
    ctx.lineWidth = w;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y + oy);
    ctx.lineTo(bx + ux * head * 0.2, by + uy * head * 0.2 + oy);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(b.x, b.y + oy);
    ctx.lineTo(bx - uy * half, by + ux * half + oy);
    ctx.lineTo(bx + uy * half, by - ux * half + oy);
    ctx.closePath();
    ctx.fill();
  }
}

function redraw() {
  const r = box.getBoundingClientRect();
  ctx.clearRect(0, 0, r.width, r.height);
  const w = Math.max(3, Math.min(r.width, r.height) / 120);
  for (const [a, b] of arrows) arrow(a, b, w);
  if (live) arrow(live[0], live[1], w);
}

function point(e) {
  const r = box.getBoundingClientRect();
  return { x: e.clientX - r.left, y: e.clientY - r.top };
}

box.addEventListener("pointerdown", (e) => {
  box.setPointerCapture(e.pointerId);
  live = [point(e), point(e)];
});
box.addEventListener("pointermove", (e) => {
  if (!live) return;
  live[1] = point(e);
  redraw();
});
const end = () => {
  if (!live) return;
  if (Math.hypot(live[1].x - live[0].x, live[1].y - live[0].y) > 8) {
    arrows.push(live);
    clear.hidden = false;
    hint.textContent = "That's the arrow. In the app, ↩ would copy all of this.";
  }
  live = null;
  redraw();
};
box.addEventListener("pointerup", end);
box.addEventListener("pointercancel", end);
clear.addEventListener("click", () => {
  arrows.length = 0;
  clear.hidden = true;
  redraw();
});
addEventListener("resize", fit);
fit();
