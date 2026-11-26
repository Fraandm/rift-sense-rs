import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import test from "node:test";

test("overlay muestra el legendario y sus alternativas, no la compra pequeña", async () => {
  const elements = new Map();
  function element(id) {
    if (!elements.has(id)) {
      elements.set(id, {
        textContent: "",
        classList: { add() {}, remove() {}, toggle() {} },
        removeAttribute() {},
        replaceChildren() { this.textContent = ""; },
        append(...nodes) { this.textContent += nodes.map((node) => typeof node === "string" ? node : node.textContent).join(""); },
      });
    }
    return elements.get(id);
  }
  const requests = [];
  const context = {
    document: {
      getElementById: element,
      createElement: () => element("created"),
      createTextNode: (value) => value,
    },
    fetch: async (url) => {
      requests.push(url);
      return {
        ok: true,
        json: async () =>
          url === "/api/health"
            ? { patch: "16.23.1" }
            : {
                nextPurchase: { id: 3020, name: "Botas", icon: "3020.png" },
                nextLegendary: { id: 3118, name: "Corazón de acero", missingGold: 1000 },
                legendaryOptions: [{ id: 1, name: "Alternativa A", missingGold: 1200 }, { id: 2, name: "Alternativa B", affordable: true }],
                items: [{ id: 3118, name: "Otro objeto" }],
                champion: { name: "Ahri" },
              },
      };
    },
    AbortController,
    setTimeout: () => 1,
    clearTimeout() {},
    Intl,
  };
  vm.runInNewContext(await readFile(new URL("../web/assets/js/overlay.js", import.meta.url), "utf8"), context);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(element("item-name").textContent, "Corazón de acero");
  assert.match(element("option-one").textContent, /Alternativa A/);
  assert.match(element("option-two").textContent, /Alternativa B/);
  assert.deepEqual(requests, ["/api/health", "/api/overlay/snapshot"]);
});
