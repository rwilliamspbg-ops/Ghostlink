import fs from 'node:fs/promises';
import path from 'node:path';
import { chromium } from 'playwright';

const OUT_DIR = path.resolve(process.cwd(), '../../..', 'docs', 'screenshots', 'ui-polish');
const BASE_URL = process.env.GHOSTLINK_UI_PREVIEW_URL || 'http://127.0.0.1:4173/?mock=1';

async function capture() {
  await fs.mkdir(OUT_DIR, { recursive: true });

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1720, height: 1040 } });
  const page = await context.newPage();

  await page.goto(BASE_URL, { waitUntil: 'networkidle' });

  await page.screenshot({ path: path.join(OUT_DIR, '01-home.png'), fullPage: true });

  await page.getByRole('button', { name: /Chat/i }).first().click();
  await page.waitForTimeout(200);
  await page.screenshot({ path: path.join(OUT_DIR, '02-chat.png'), fullPage: true });

  await page.getByRole('button', { name: /Cluster/i }).first().click();
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(OUT_DIR, '03-cluster.png'), fullPage: true });

  await page.setViewportSize({ width: 430, height: 932 });
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(OUT_DIR, '04-mobile-home.png'), fullPage: true });

  await browser.close();
}

capture().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
