// Isolated gateway; controlled network responses and browser time exercise slow polling.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const {spawn} = require('node:child_process');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

(async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'rcpa-dashboard-refresh-'));
    const log = fs.openSync(path.join(directory, 'server.log'), 'a');
    const reservation = http.createServer();
    await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
    const port = reservation.address().port;
    await new Promise(resolve => reservation.close(resolve));
    const base = `http://127.0.0.1:${port}`;
    let gateway, browser, release;
    try {
        gateway = spawn(path.resolve('target/debug/rcpa'), ['--data-dir', directory, '--token', 'refresh-fixture', '--port', String(port)], {stdio: ['ignore', log, log]});
        const deadline = Date.now() + 30000;
        while (true) {
            if (gateway.exitCode !== null) throw new Error(fs.readFileSync(path.join(directory, 'server.log'), 'utf8'));
            try { if ((await fetch(base + '/health')).ok) break; } catch {}
            if (Date.now() > deadline) throw new Error('Gateway startup timed out');
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        browser = await chromium.launch({headless: true, ...(process.env.RCPA_BROWSER_PATH ? {executablePath: process.env.RCPA_BROWSER_PATH} : {})});
        const context = await browser.newContext();
        assert((await context.request.post(base + '/v1/admin/login', {data: {token: 'refresh-fixture'}})).ok());
        const page = await context.newPage();
        const errors = [];
        page.on('pageerror', error => errors.push(error.message));
        await page.clock.install();
        await page.goto(base + '/dashboard');
        await page.waitForFunction(() => document.getElementById('analytics-feedback').textContent.startsWith('已更新'));
        await page.clock.pauseAt(new Date(Date.now() + 1000));

        let requests = 0, hold = true, markStarted;
        const started = new Promise(resolve => { markStarted = resolve; });
        const gate = new Promise(resolve => { release = resolve; });
        await page.route('**/v1/admin/analytics/dashboard?*', async route => {
            requests++;
            const response = await route.fetch();
            markStarted();
            if (hold) await gate;
            await route.fulfill({response});
        });
        await page.evaluate(() => { void loadAnalytics(); });
        await started;
        await page.clock.runFor(15000);
        assert.equal(requests, 1, 'Three polling ticks must not replace a pending query');
        assert(await page.evaluate(() => !analyticsRequest.signal.aborted));
        hold = false;
        release();
        await page.waitForFunction(() => analyticsRequest === null);
        await page.clock.runFor(5000);
        await page.waitForFunction(() => analyticsRequest === null);
        assert.equal(requests, 2, 'Polling must resume after the slow query completes');

        // User changes still supersede a pending request and render the new range.
        let releaseOld, markOldStarted;
        const oldStarted = new Promise(resolve => { markOldStarted = resolve; });
        const oldGate = new Promise(resolve => { releaseOld = resolve; });
        await page.unrouteAll({behavior: 'wait'});
        let first = true;
        await page.route('**/v1/admin/analytics/dashboard?*', async route => {
            const response = await route.fetch();
            if (first) { first = false; markOldStarted(); await oldGate; }
            await route.fulfill({response}).catch(() => {}); // Superseded request is aborted.
        });
        await page.evaluate(() => { void loadAnalytics(); window.oldAnalyticsRequest = analyticsRequest; });
        await oldStarted;
        await page.selectOption('#time-range', '6h');
        await page.waitForFunction(() => oldAnalyticsRequest.signal.aborted && analyticsRequest === null);
        releaseOld();
        assert.equal(await page.locator('#time-range').inputValue(), '6h');
        assert.deepEqual(errors, []);
        console.log('Dashboard slow-refresh regression passed. Fixture:', directory);
    } finally {
        release?.();
        if (browser) await browser.close();
        if (gateway && gateway.exitCode === null) {
            gateway.kill('SIGINT');
            await new Promise(resolve => gateway.once('exit', resolve));
        }
        fs.closeSync(log);
    }
})().catch(error => { console.error(error); process.exitCode = 1; });
