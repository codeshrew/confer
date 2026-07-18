import { test, expect } from '@playwright/test';

test('the dashboard shell loads and shows the confer wordmark', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('confer')).toBeVisible();
});
