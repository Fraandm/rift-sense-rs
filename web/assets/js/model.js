/* Vista de modelo: métricas, curvas y calibración. */

import { api } from "./api.js";
import { barGroupChart, lineChart } from "./charts.js";
import { $, emptyState, num, pct } from "./ui.js";

let loaded = false;

function metric(label, value, note, variant = "") {
  return `<div class="metric ${variant}">
    <span>${label}</span>
    <strong>${value}</strong>
    <small>${note}</small>
  </div>`;
}

function renderMetrics(draft, build) {
  const host = $("#model-metrics");
  if (!host) return;
  const d = draft?.report;
  const b = build?.report;
  const draftTest = d?.test;
  const buildTest = b?.test;
  const params = (d?.parameters || 0) + (b?.parameters || 0);
  const samples = d?.samples ? d.samples.train + d.samples.val + d.samples.test : 0;

  host.innerHTML = [
    metric("AUC DraftNet", draftTest ? draftTest.auc.toFixed(3) : "—", draftTest ? `test · ${num(draftTest.n)} drafts` : "sin entrenar", "teal"),
    metric("Precisión draft", draftTest ? pct(draftTest.accuracy, 1) : "—", "clasificación binaria", ""),
    metric("Top-3 BuildNet", buildTest ? pct(buildTest.top3, 1) : "—", buildTest ? `test · ${num(buildTest.n)} muestras` : "sin entrenar", "blue"),
    metric("Top-1 BuildNet", buildTest ? pct(buildTest.top1, 1) : "—", buildTest ? `baseline ${pct(buildTest.popularityBaselineTop1, 1)}` : "sin entrenar", ""),
    metric("Parámetros", params ? num(params) : "—", "suma de ambas redes", "plain"),
    metric("Muestras de draft", samples ? num(samples) : "—", "train + val + test", "plain"),
  ].join("");
}

function renderCharts(draft, build) {
  const dHistory = draft?.report?.history || [];
  lineChart($("#chart-draft"), {
    labels: dHistory.map((h) => `E${h.epoch}`),
    series: [
      { name: "Val loss", color: "#c8aa6e", values: dHistory.map((h) => h.val_logloss), axis: "left" },
      { name: "AUC", color: "#0ac8b9", values: dHistory.map((h) => h.val_auc), axis: "right" },
    ],
    height: 230,
    format: (v) => Number(v).toFixed(2),
    ariaLabel: "Curvas de aprendizaje de DraftNet",
  });

  const bHistory = build?.report?.history || [];
  lineChart($("#chart-build"), {
    labels: bHistory.map((h) => `E${h.epoch}`),
    series: [
      { name: "Top-1", color: "#c8aa6e", values: bHistory.map((h) => h.val_top1), axis: "left" },
      { name: "Top-3", color: "#0ac8b9", values: bHistory.map((h) => h.val_top3), axis: "right" },
    ],
    height: 230,
    format: (v) => `${(Number(v) * 100).toFixed(0)}%`,
    ariaLabel: "Acierto de BuildNet por epoch",
  });

  const bins = draft?.report?.test?.calibration || [];
  barGroupChart($("#chart-calibration"), {
    labels: bins.map((b) => `${Math.round(b.lo * 100)}–${Math.round(b.hi * 100)}`),
    series: [
      { name: "Predicho", color: "#c8aa6e", values: bins.map((b) => b.pred) },
      { name: "Real", color: "#0ac8b9", values: bins.map((b) => b.actual) },
    ],
    height: 230,
    ariaLabel: "Calibración de DraftNet",
  });

  if (!bins.length && $("#chart-calibration")) {
    $("#chart-calibration").innerHTML = emptyState("i-info", "Sin calibración", "El reporte de test no contiene bins.");
  }
}

function renderTable(draft, build) {
  const tbody = $("#model-table tbody");
  if (!tbody) return;
  const rows = [];
  const push = (model, split, label, value, n) => {
    rows.push(`<tr><td>${model}</td><td>${split}</td><td>${label}</td><td>${value}</td><td>${num(n)}</td></tr>`);
  };
  const d = draft?.report;
  const b = build?.report;
  if (d?.test) {
    push("DraftNet", "test", "AUC", d.test.auc.toFixed(4), d.test.n);
    push("DraftNet", "test", "Accuracy", pct(d.test.accuracy, 2), d.test.n);
    push("DraftNet", "test", "Log loss", d.test.logloss.toFixed(4), d.test.n);
    push("DraftNet", "test", "Brier", d.test.brier.toFixed(4), d.test.n);
  }
  if (d?.partialDraft) {
    for (const [key, value] of Object.entries(d.partialDraft)) {
      const label = key.replace("_unknown", " picks desconocidos");
      push("DraftNet", "parcial", `AUC · ${label}`, value.auc.toFixed(4), value.n);
    }
  }
  if (b?.test) {
    push("BuildNet", "test", "Top-1", pct(b.test.top1, 2), b.test.n);
    push("BuildNet", "test", "Top-3", pct(b.test.top3, 2), b.test.n);
    push("BuildNet", "test", "Top-5", pct(b.test.top5, 2), b.test.n);
    push("BuildNet", "test", "MRR", b.test.mrr.toFixed(4), b.test.n);
  }
  tbody.innerHTML = rows.join("") || `<tr><td colspan="5">Sin reportes de evaluación.</td></tr>`;

  const fine = $("#model-fineprint");
  if (fine) {
    const bits = [];
    if (d) {
      bits.push(
        `DraftNet: ${d.epochsRun ?? "?"} epochs (mejor ${d.bestEpoch ?? "?"}), ${num(d.parameters || 0)} parámetros, ${
          d.trainingMinutes ?? "?"
        } min de entrenamiento.`
      );
    }
    if (b) {
      bits.push(
        `BuildNet: ${b.epochsRun ?? "?"} epochs (mejor ${b.bestEpoch ?? "?"}), ${num(b.parameters || 0)} parámetros, ${
          b.trainingMinutes ?? "?"
        } min. Filtro de parche >= ${b.minPatch || "?"}.`
      );
    } else {
      bits.push("BuildNet aún no entrenado.");
    }
    bits.push("Split cronologico: train 85% · val 10% · test 5%. Las métricas de test no se usan para elegir el checkpoint.");
    if (!d && !b) bits.push("Aún no hay modelos entrenados. Ejecuta las instrucciones del README para generarlos.");
    fine.textContent = bits.join(" ");
  }
}

function render(data) {
  const device = $("#model-device");
  if (device) device.textContent = `${(data.device || "cpu").toUpperCase()} · ${data.patch || ""}`;
  renderMetrics(data.draft, data.build);
  renderCharts(data.draft, data.build);
  renderTable(data.draft, data.build);

  if (!data.draft?.exists && !data.build?.exists) {
    const host = $("#model-metrics");
    if (host) {
      host.insertAdjacentHTML(
        "afterend",
        emptyState("i-brain", "Todavía no hay artefactos", "Entrena las redes para activar las recomendaciones del modelo.")
      );
    }
  }
}

export async function load() {
  if (loaded) return;
  loaded = true;
  try {
    const data = await api.get("/api/model");
    render(data);
  } catch (err) {
    loaded = false;
    const host = $("#model-metrics");
    if (host) host.innerHTML = emptyState("i-info", "No se pudo cargar el modelo", err.message);
  }
}
