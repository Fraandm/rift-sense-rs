"""Smoke test de app y overlay con el ejemplo local, sin cliente del juego."""
import os
from playwright.sync_api import sync_playwright

BASE = os.environ.get("RIFTSENSE_TEST_URL", "http://127.0.0.1:8491")


def main():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(channel="msedge", headless=True)
        context = browser.new_context()
        page = context.new_page()
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(BASE + "/", wait_until="domcontentloaded", timeout=30000)
        close = page.get_by_role("button", name="Cerrar guía")
        if close.is_visible():
            close.click()
        page.locator('[data-tab="live"]').first.click()
        page.locator("#live-example").click()
        page.wait_for_function(
            "async () => {const r=await fetch('/api/overlay/snapshot');"
            "const b=await r.json();return !!b?.nextLegendary}", timeout=45000
        )
        snapshot = page.evaluate("async()=>await (await fetch('/api/overlay/snapshot')).json()")
        name = snapshot["nextLegendary"]["name"]
        assert name in page.locator("#live-legendary").inner_text()
        overlay = context.new_page()
        overlay.on("pageerror", lambda error: errors.append(str(error)))
        overlay.goto(BASE + "/overlay.html", wait_until="domcontentloaded")
        overlay.wait_for_function(
            "(name) => document.querySelector('#item-name')?.textContent === name",
            arg=name, timeout=20000,
        )
        for index, option in enumerate(snapshot.get("legendaryOptions", [])[:2]):
            assert option["name"] in overlay.locator(["#option-one", "#option-two"][index]).inner_text()
        assert not errors, errors
        print(f"App y overlay coinciden: {name}")
        browser.close()


if __name__ == "__main__":
    main()
