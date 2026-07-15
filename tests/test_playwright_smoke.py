import re

from playwright.sync_api import Page, expect


def test_ghostlink_homepage_smoke(page: Page):
    page.goto("http://127.0.0.1:8000", wait_until="networkidle")

    expect(page).to_have_title(re.compile("Ghostlink"))
    expect(page.locator("h1")).to_contain_text("Ghostlink")
    expect(page.get_by_role("link", name="View on GitHub")).to_be_visible()
    expect(page.get_by_text("Proof points")).to_be_visible()
    expect(page.get_by_text("Commercial path")).to_be_visible()
    expect(page.get_by_text("See Ghostlink in action")).to_be_visible()
    expect(page.get_by_text("Run the demo in 3 steps")).to_be_visible()
    expect(page.get_by_text("What the terminal output looks like")).to_be_visible()
