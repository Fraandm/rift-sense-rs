"""Prueba Lab en un servidor aislado; modifica sus ajustes locales."""
import os
from playwright.sync_api import sync_playwright

base = os.environ["RIFTSENSE_TEST_URL"]
with sync_playwright() as p:
    browser = p.chromium.launch(channel="msedge", headless=True)
    page = browser.new_page(viewport={"width": 1440, "height": 900})
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.goto(base, wait_until="domcontentloaded")
    page.locator('[data-tab="settings"]').first.click()
    lab = page.locator("#settings-lab")
    lab.wait_for(state="attached")
    page.wait_for_function("() => !document.querySelector('#settings-lab').disabled")
    assert not lab.is_checked()
    request = {"champion": "Ahri", "role": "MID", "mode": "classic",
               "gold": 1800, "owned": [], "stable": True, "simulate": False}
    def recommend():
        return page.evaluate("""async req => {
          const r = await fetch('/api/build', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(req)});
          if (!r.ok) throw Error(await r.text());
          return r.json();
        }""", request)
    standard = recommend()
    assert standard["buildModel"] == "standard"
    page.locator('label:has(#settings-lab) .track').click()
    page.wait_for_function("async () => (await (await fetch('/api/settings')).json()).labEnabled")
    experimental = recommend()
    assert experimental["buildModel"] == "lab"
    assert [(x["id"], x["score"]) for x in experimental["items"]] != [
        (x["id"], x["score"]) for x in standard["items"]
    ], "Lab no cambió las puntuaciones de la prueba"
    page.reload(wait_until="domcontentloaded")
    page.locator('[data-tab="settings"]').first.click()
    page.wait_for_function("() => document.querySelector('#settings-lab').checked")
    page.locator('label:has(#settings-lab) .track').click()
    page.wait_for_function("async () => !(await (await fetch('/api/settings')).json()).labEnabled")
    restored = recommend()
    assert restored["buildModel"] == "standard"
    assert [(x["id"], x["score"]) for x in restored["items"]] == [
        (x["id"], x["score"]) for x in standard["items"]
    ]
    assert not errors, errors
    print("Lab: alternancia, persistencia y vuelta al habitual verificadas")
    browser.close()
