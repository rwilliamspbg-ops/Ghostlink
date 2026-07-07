from playwright.sync_api import sync_playwright
import os

def run_cuj(page):
    print("Navigating to http://localhost:3000...")
    page.goto("http://localhost:3000")
    page.wait_for_timeout(3000)

    # 1. Models Tab
    print("Checking Models tab...")
    page.get_by_role("button", name="Models").click()
    page.wait_for_timeout(1000)
    page.screenshot(path="/home/jules/verification/screenshots/models_tab.png")

    # 2. Chat Tab - Select Model and send message
    print("Performing chat...")
    page.get_by_role("button", name="Chat").click()
    page.wait_for_timeout(1000)

    # Select first available model
    page.get_by_role("combobox").select_option(index=1)
    page.wait_for_timeout(500)

    page.get_by_placeholder("Enter your message...").fill("Hello Ghostlink! Tell me about yourself.")
    page.wait_for_timeout(500)

    page.get_by_role("button", name="Send Message").click()
    print("Message sent, waiting for response...")
    page.wait_for_timeout(5000)

    page.screenshot(path="/home/jules/verification/screenshots/verification.png")

    # 3. Metrics Tab
    print("Checking Metrics tab...")
    page.get_by_role("button", name="Metrics").click()
    page.wait_for_timeout(2000)
    page.screenshot(path="/home/jules/verification/screenshots/metrics_tab.png")

    print("Verification complete.")

if __name__ == "__main__":
    os.makedirs("/home/jules/verification/videos", exist_ok=True)
    os.makedirs("/home/jules/verification/screenshots", exist_ok=True)

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        context = browser.new_context(
            record_video_dir="/home/jules/verification/videos"
        )
        page = context.new_page()
        try:
            run_cuj(page)
        finally:
            context.close()
            browser.close()
