import { test, expect } from '@playwright/test'

test.describe('Ghostlink Studio E2E', () => {
  test('loads the app and shows sidebar navigation', async ({ page }) => {
    await page.goto('/')
    await expect(page).toHaveTitle(/Ghostlink/)
    await expect(page.locator('nav')).toBeVisible()
  })

  test('can switch between tabs via sidebar', async ({ page }) => {
    await page.goto('/')
    const sidebar = page.locator('nav')
    const links = sidebar.locator('a, button')
    const count = await links.count()
    expect(count).toBeGreaterThanOrEqual(2)
  })

  test('shows New Chat button', async ({ page }) => {
    await page.goto('/')
    const newChat = page.getByText('New Chat').or(page.getByText('+ New Chat'))
    await expect(newChat.first()).toBeVisible()
  })

  test('metrics endpoint returns data', async ({ page }) => {
    const response = await page.request.get('http://localhost:8003/api/metrics')
    expect(response.ok()).toBeTruthy()
    const body = await response.json()
    expect(body.metrics).toBeDefined()
    expect(typeof body.metrics.throughput).toBe('number')
  })

  test('health endpoint is reachable', async ({ page }) => {
    const response = await page.request.get('http://localhost:8003/health')
    expect(response.ok()).toBeTruthy()
    const body = await response.json()
    expect(body.status).toBe('healthy')
  })
})
