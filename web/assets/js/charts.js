/* Graficos SVG ligeros, sin dependencias externas. */

const NS = "http://www.w3.org/2000/svg";

function svgEl(name, attrs = {}) {
  const el = document.createElementNS(NS, name);
  for (const [key, value] of Object.entries(attrs)) {
    if (value != null) el.setAttribute(key, String(value));
  }
  return el;
}

function empty(container, text) {
  container.innerHTML = `<div class="empty-state"><svg class="icon"><use href="#i-brain"/></svg><strong>${text}</strong></div>`;
}

export function lineChart(container, { series = [], labels = [], height = 230, format = (v) => Number(v).toFixed(3), ariaLabel = "Grafico de lineas" } = {}) {
  if (!container) return;
  container.innerHTML = "";
  const cleanSeries = series.filter((s) => Array.isArray(s.values) && s.values.length);
  const n = Math.min(labels.length, ...cleanSeries.map((s) => s.values.length));
  if (!cleanSeries.length || n < 2) {
    empty(container, "Sin historial de entrenamiento");
    return;
  }

  const W = 640;
  const H = height;
  const m = { left: 50, right: 52, top: 14, bottom: 30 };
  const pw = W - m.left - m.right;
  const ph = H - m.top - m.bottom;

  const resolved = cleanSeries.map((s, index) => ({
    ...s,
    axis: s.axis || (index === 0 ? "left" : "right"),
    values: s.values.slice(0, n),
  }));

  const domains = {};
  for (const s of resolved) {
    const vals = s.values.filter((v) => Number.isFinite(v));
    if (!vals.length) continue;
    let min = Math.min(...vals);
    let max = Math.max(...vals);
    let range = max - min;
    if (range < 1e-9) range = Math.abs(max) * 0.1 || 0.1;
    min -= range * 0.1;
    max += range * 0.1;
    if (!domains[s.axis]) domains[s.axis] = { min, max };
    else {
      domains[s.axis].min = Math.min(domains[s.axis].min, min);
      domains[s.axis].max = Math.max(domains[s.axis].max, max);
    }
  }

  const x = (i) => m.left + (pw * i) / (n - 1);
  const y = (axis, v) => {
    const d = domains[axis] || { min: 0, max: 1 };
    return m.top + ph * (1 - (v - d.min) / (d.max - d.min || 1));
  };

  const svg = svgEl("svg", {
    viewBox: `0 0 ${W} ${H}`,
    role: "img",
    "aria-label": ariaLabel,
    preserveAspectRatio: "xMidYMid meet",
  });

  // Grid + etiquetas
  const ticks = 4;
  for (let i = 0; i <= ticks; i += 1) {
    const gy = m.top + (ph * i) / ticks;
    svg.append(
      svgEl("line", { x1: m.left, x2: W - m.right, y1: gy, y2: gy, class: "grid-line", "vector-effect": "non-scaling-stroke" })
    );
  }

  const leftSeries = resolved.find((s) => s.axis === "left");
  const rightSeries = resolved.find((s) => s.axis === "right");
  const axisLabel = (axis, color, textAnchor, xPos) => {
    const d = domains[axis];
    if (!d) return;
    for (let i = 0; i <= ticks; i += 1) {
      const value = d.max - ((d.max - d.min) * i) / ticks;
      const gy = m.top + (ph * i) / ticks;
      const text = svgEl("text", {
        x: xPos,
        y: gy + 3,
        class: "axis-text",
        "text-anchor": textAnchor,
        fill: color,
      });
      text.textContent = format(value);
      svg.append(text);
    }
  };
  if (leftSeries) axisLabel("left", leftSeries.color || "#c8aa6e", "end", m.left - 8);
  if (rightSeries) axisLabel("right", rightSeries.color || "#0ac8b9", "start", W - m.right + 8);

  const step = Math.max(1, Math.ceil(n / 6));
  for (let i = 0; i < n; i += step) {
    const text = svgEl("text", {
      x: x(i),
      y: H - 8,
      class: "axis-text",
      "text-anchor": i === 0 ? "start" : i === n - 1 ? "end" : "middle",
    });
    text.textContent = labels[i];
    svg.append(text);
  }

  // Series
  const paths = [];
  for (const s of resolved) {
    const points = s.values.map((v, i) => [x(i), y(s.axis, v)]).filter(([, yy]) => Number.isFinite(yy));
    if (points.length < 2) continue;
    const d = points.map(([xx, yy], i) => `${i === 0 ? "M" : "L"}${xx.toFixed(2)},${yy.toFixed(2)}`).join(" ");
    if (s.area) {
      const areaD = `${d} L${points[points.length - 1][0].toFixed(2)},${m.top + ph} L${points[0][0].toFixed(2)},${m.top + ph} Z`;
      svg.append(svgEl("path", { d: areaD, fill: s.color || "#c8aa6e", class: "series-area" }));
    }
    const path = svgEl("path", {
      d,
      class: "series-line",
      stroke: s.color || "#c8aa6e",
      "vector-effect": "non-scaling-stroke",
    });
    svg.append(path);
    paths.push({ series: s, points, path });
  }

  // Hover
  const hover = svgEl("g", { opacity: "0" });
  const hoverLine = svgEl("line", { y1: m.top, y2: m.top + ph, class: "hover-line", "vector-effect": "non-scaling-stroke" });
  hover.append(hoverLine);
  const dots = paths.map(({ series: s }) => {
    const dot = svgEl("circle", { r: 4, fill: s.color || "#c8aa6e", class: "dot-hover" });
    hover.append(dot);
    return dot;
  });
  svg.append(hover);

  const zone = svgEl("rect", {
    x: m.left,
    y: m.top,
    width: pw,
    height: ph,
    fill: "transparent",
    style: "cursor: crosshair",
  });
  svg.append(zone);
  container.append(svg);

  const tooltip = document.createElement("div");
  tooltip.className = "chart-tooltip";
  tooltip.style.display = "none";
  container.append(tooltip);

  const show = (event) => {
    const rect = container.getBoundingClientRect();
    const svgRect = svg.getBoundingClientRect();
    const relX = event.clientX - svgRect.left;
    const px = (relX / svgRect.width) * W;
    const ratio = Math.max(0, Math.min(1, (px - m.left) / pw));
    const index = Math.round(ratio * (n - 1));
    hover.setAttribute("opacity", "1");
    hoverLine.setAttribute("x1", x(index));
    hoverLine.setAttribute("x2", x(index));
    paths.forEach(({ series: s, points }, i) => {
      const point = points[index];
      if (!point) return;
      dots[i].setAttribute("cx", point[0]);
      dots[i].setAttribute("cy", point[1]);
    });
    const rows = paths
      .map(({ series: s }) => `<span style="color:${s.color || "#c8aa6e"}">●</span> ${s.name}: ${format(s.values[index])}`)
      .join("<br>");
    tooltip.innerHTML = `<strong>${labels[index]}</strong><br>${rows}`;
    tooltip.style.display = "block";
    const svgPointY = Math.min(
      ...paths.map(({ series: s }) => y(s.axis, s.values[index])).filter(Number.isFinite)
    );
    tooltip.style.left = `${(x(index) / W) * svgRect.width + (svgRect.left - rect.left)}px`;
    tooltip.style.top = `${(svgPointY / H) * svgRect.height + (svgRect.top - rect.top)}px`;
  };

  zone.addEventListener("mousemove", show);
  zone.addEventListener("mouseleave", () => {
    hover.setAttribute("opacity", "0");
    tooltip.style.display = "none";
  });
}

export function barGroupChart(container, { labels = [], series = [], height = 230, ariaLabel = "Grafico de barras" } = {}) {
  if (!container) return;
  container.innerHTML = "";
  if (!labels.length || !series.length) {
    empty(container, "Sin datos de calibración");
    return;
  }
  const W = 640;
  const H = height;
  const m = { left: 42, right: 12, top: 16, bottom: 34 };
  const pw = W - m.left - m.right;
  const ph = H - m.top - m.bottom;

  const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": ariaLabel, preserveAspectRatio: "xMidYMid meet" });

  for (let i = 0; i <= 4; i += 1) {
    const gy = m.top + (ph * i) / 4;
    svg.append(svgEl("line", { x1: m.left, x2: W - m.right, y1: gy, y2: gy, class: "grid-line", "vector-effect": "non-scaling-stroke" }));
    const text = svgEl("text", { x: m.left - 7, y: gy + 3, class: "axis-text", "text-anchor": "end" });
    text.textContent = `${Math.round(100 - i * 25)}%`;
    svg.append(text);
  }

  const groupW = pw / labels.length;
  const barW = Math.min(16, (groupW * 0.6) / series.length);
  labels.forEach((label, i) => {
    const center = m.left + groupW * (i + 0.5);
    series.forEach((s, si) => {
      const value = Math.max(0, Math.min(1, Number(s.values[i]) || 0));
      const bh = ph * value;
      const bx = center - (barW * series.length) / 2 + si * barW;
      const rect = svgEl("rect", {
        x: bx,
        y: m.top + ph - bh,
        width: barW - 1,
        height: Math.max(bh, 0.5),
        fill: s.color || "#c8aa6e",
        rx: 1.5,
        opacity: 0.9,
      });
      const title = svgEl("title");
      title.textContent = `${label} · ${s.name}: ${(value * 100).toFixed(1)}%`;
      rect.append(title);
      svg.append(rect);
    });
    const text = svgEl("text", {
      x: center,
      y: H - 12,
      class: "axis-text",
      "text-anchor": "middle",
    });
    text.textContent = label;
    svg.append(text);
  });

  container.append(svg);
}
