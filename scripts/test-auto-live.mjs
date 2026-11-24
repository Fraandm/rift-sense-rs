import { test } from "node:test";
import assert from "node:assert/strict";
import { createAutoLiveTransition } from "../web/assets/js/auto-live.js";

test("navega al arrancar, incluso durante la pantalla de carga", () => {
  const observe = createAutoLiveTransition();
  assert.equal(observe({ phase: "ChampSelect" }), false);
  assert.equal(observe({ phase: "GameStart" }), true);
  assert.equal(observe({ phase: "InProgress", inGame: true }, true, 10), false);
});

test("no interrumpe la navegación manual en sondeos posteriores", () => {
  const observe = createAutoLiveTransition();
  assert.equal(observe({ inGame: true }, true, 10), true);
  for (let i = 0; i < 10; i++) {
    assert.equal(observe({ inGame: true }, true, 10), false);
  }
});

test("una desconexión o reconexión no vuelve a imponer la pestaña", () => {
  const observe = createAutoLiveTransition();
  assert.equal(observe({ liveClient: true }, true, 10), true);
  assert.equal(observe({ phase: null, connected: false }), false);
  assert.equal(observe({ phase: "None", connected: false }), false);
  assert.equal(observe({ phase: "Reconnect" }, true, 10), false);
});

test("otra partida vuelve a activar la transición", () => {
  const observe = createAutoLiveTransition();
  assert.equal(observe({ inGame: true }, true, 10), true);
  assert.equal(observe({ phase: "EndOfGame" }), false);
  assert.equal(observe({ phase: "Lobby" }), false);
  assert.equal(observe({ phase: "GameStart" }), true);
  assert.equal(observe({ inGame: true }, true, 11), false);
});

test("detecta otro gameId aunque se pierda el lobby", () => {
  const observe = createAutoLiveTransition();
  assert.equal(observe({ liveClient: true }, true, 10), true);
  assert.equal(observe({ liveClient: true }, true, 11), true);
  assert.equal(observe({ liveClient: true }, true, 11), false);
});

test("respeta Auto desactivado y admite abrir la app con partida en curso", () => {
  const observe = createAutoLiveTransition();
  assert.equal(observe({ inGame: true }, false, 10), false);
  assert.equal(observe({ inGame: true }, true, 10), true);
  assert.equal(observe({ inGame: true }, true, 10), false);
});
