// Offline design regression. This never starts or contacts the gateway.
// PLAYWRIGHT_MODULE=/path/to/playwright RCPA_BROWSER_PATH=/path/to/chromium node tests/performance_design_browser.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

(async () => {
    const artifacts = fs.mkdtempSync(path.join(os.tmpdir(), 'rcpa-performance-design-'));
    const browser = await chromium.launch({headless: true, ...(process.env.RCPA_BROWSER_PATH ? {executablePath: process.env.RCPA_BROWSER_PATH} : {})});
    try {
        const page = await browser.newPage({viewport: {width: 1440, height: 960}, reducedMotion: 'reduce'});
        const errors = [], network = [];
        page.on('pageerror', e => errors.push(e.message));
        page.on('request', request => {if (/^https?:/.test(request.url())) network.push(request.url());});
        await page.goto(pathToFileURL(path.resolve('docs/design/performance-analytics.html')).href);
        assert(await page.locator('#overview-panel').isVisible());
        assert.equal(await page.locator('#home-metrics .metric').count(), 6);
        assert.equal(await page.locator('#home-table-body tr').count(), 6);
        assert.equal(await page.locator('.home-trends .panel').count(), 2);
        await page.locator('#home-health-actions button').filter({hasText:'异常'}).click();
        assert(await page.locator('#home-table-body tr').allInnerTexts().then(rows=>rows.length>0&&rows.every(row=>row.includes('异常'))));
        await page.locator('#home-all-channels').click();
        assert.equal(await page.locator('#home-table-body tr').count(), 6);
        assert(await page.evaluate(()=>{
            const row=document.querySelector('#home-table-body tr'),value=row.querySelector('.model-name');
            const range=document.createRange();range.selectNodeContents(value);
            const selection=window.getSelection();selection.removeAllRanges();selection.addRange(range);
            const selectedText=selection.toString();render();
            const stable=row===document.querySelector('#home-table-body tr')&&selectedText===selection.toString();
            selection.removeAllRanges();return stable;
        }));
        await page.selectOption('#home-order','health');
        assert(await page.locator('#home-table-body tr').first().innerText().then(s=>s.includes('异常')));
        const homeRow=page.locator('#home-table-body tr').first();
        await homeRow.focus();await page.keyboard.press('Enter');
        assert(await page.locator('#detail-dialog').isVisible());
        await page.keyboard.press('Escape');
        assert(await homeRow.evaluate(n=>document.activeElement===n));
        await page.locator('[data-chart-table="home-latency"]').click();
        assert.equal(await page.locator('#data-body tbody tr').count(),12);
        await page.keyboard.press('Escape');
        await page.locator('#go-performance').click();
        assert.equal(await page.locator('#metric-strip .metric').count(), 6);
        assert.equal(await page.locator('#table-body tr').count(), 6);
        assert(await page.locator('#table-body').innerText().then(s => s.includes('未观测') && s.includes('样本不足')));

        const initial = await page.evaluate(() => ({requests: snapshot.total.requests, timed: snapshot.total.timed.length, speed: snapshot.total.speed.length, success: snapshot.total.success}));
        assert(initial.requests > 100 && initial.timed > 0 && initial.speed > 0 && initial.speed <= initial.timed);
        assert(initial.success < 1, 'Demo must include non-complete requests, including HTTP 200 errors.');
        assert(await page.evaluate(() => snapshot.total.http200 > 0));

        // Identical refreshes preserve table identity, selection, ordering, and reading position.
        assert(await page.evaluate(() => {
            const row = document.querySelector('#table-body tr');
            const text = row.querySelector('.model-name');
            const range = document.createRange(); range.selectNodeContents(text);
            const selection = window.getSelection(); selection.removeAllRanges(); selection.addRange(range);
            const selectedText = selection.toString();
            const scroller = document.getElementById('table-scroll'); scroller.scrollTop = 20;
            const top = scroller.scrollTop; render();
            const stable = row === document.querySelector('#table-body tr') && selectedText === selection.toString() && top === scroller.scrollTop;
            selection.removeAllRanges(); return stable;
        }));

        await page.locator('#model-filter summary').click();
        await page.locator('#model-options input[value="gpt-6.1-sol"]').check();
        await page.locator('#model-options input[value="deepseek-flash"]').check();
        await page.locator('#provider-filter summary').click();
        await page.locator('#provider-options input[value="备用渠道"]').check();
        assert.equal(await page.locator('#table-body tr').count(), 2);
        assert(await page.evaluate(() => snapshot.rows.every(r => ['gpt-6.1-sol', 'deepseek-flash'].includes(r.model) && r.provider === '备用渠道')));
        await page.locator('#reset').click();
        assert.equal(await page.locator('#table-body tr').count(), 6);

        await page.locator('#more-filter summary').click();
        await page.selectOption('#stream', 'nonstream');
        assert.equal(await page.locator('#metric-strip .metric strong').nth(2).textContent(), '—');
        assert.equal(await page.locator('#metric-strip .metric strong').nth(3).textContent(), '—');
        assert(await page.evaluate(() => snapshot.total.timed.length === 0 && snapshot.total.speed.length === 0));
        await page.locator('#reset').click();

        // Targets are explicit and optional; missing targets never produce healthy/SLO values.
        await page.locator('#targets-button').click();
        await page.locator('#clear-targets').click();
        await page.locator('#targets-form button[type=submit]').click();
        assert.equal(await page.locator('#metric-strip .metric strong').nth(5).textContent(), '—');
        assert(await page.locator('#table-body').innerText().then(s => s.includes('未配置目标')));
        await page.reload();
        await page.locator('#performance-tab').click();

        // Custom windows use Shanghai time and preserve the previous window when invalid.
        await page.selectOption('#range', 'custom');
        await page.fill('#from', '2026-11-04T11:30');
        await page.fill('#to', '2026-11-04T11:00');
        await page.locator('#custom-range button[type=submit]').click();
        assert(await page.locator('#range-error').textContent());
        await page.fill('#from', '2026-10-30T10:00');
        await page.fill('#to', '2026-10-30T11:00');
        await page.locator('#custom-range button[type=submit]').click();
        assert.equal(await page.locator('#metric-strip .metric strong').first().textContent(), '0');
        assert.equal(await page.locator('#metric-strip .metric strong').nth(2).textContent(), '—');
        assert.equal(await page.evaluate(() => snapshot.total.requests), 0);
        await page.selectOption('#range', '60');

        // Native dialog stacking, keyboard tabs and focus restoration.
        const row = page.locator('#table-body tr').first();
        await row.focus(); await page.keyboard.press('Enter');
        await page.locator('#detail-dialog').waitFor({state: 'visible'});
        await page.locator('[data-detail-tab="reliability"]').click();
        assert(await page.locator('#detail-body').innerText().then(s => s.includes('HTTP 200 流内错误')));
        await page.keyboard.press('ArrowRight');
        assert.equal(await page.locator('[data-detail-tab="performance"]').getAttribute('aria-selected'), 'true');
        await page.locator('#view-logs').click();
        assert(await page.locator('#data-title').textContent().then(s => s.includes('不访问线上')));
        await page.keyboard.press('Escape');
        assert(await page.locator('#detail-dialog').isVisible());
        await page.keyboard.press('Escape');
        assert(await row.evaluate(n => document.activeElement === n));

        await page.locator('#overview-tab').click();
        assert(await page.locator('#overview-panel').isVisible());
        await page.keyboard.press('ArrowRight');
        assert(await page.locator('#performance-panel').isVisible());
        await page.locator('#table-head button').filter({hasText: 'TPS P50'}).click();
        assert.equal(await page.locator('#table-head th[aria-sort="descending"]').innerText(), 'TPS P50 ↓');

        // Chart point keyboard access and table alternative.
        await page.locator('#latency-plot .chart-point').first().focus();
        await page.keyboard.press('ArrowRight');
        assert(await page.locator('#latency-readout').textContent().then(s => s.includes('次请求')));
        await page.locator('[data-chart-table="latency"]').click();
        assert.equal(await page.locator('#data-body tbody tr').count(), 12);
        await page.keyboard.press('Escape');

        const downloadPromise = page.waitForEvent('download');
        await page.locator('#export').click();
        const download = await downloadPromise;
        const csv = path.join(artifacts, 'demo.csv'); await download.saveAs(csv);
        assert(fs.readFileSync(csv, 'utf8').includes('示例数据'));

        for (const theme of ['light', 'dark']) {
            if (await page.getAttribute('html', 'data-theme') !== theme) await page.locator('#theme-toggle').click();
            await page.locator('#theme-toggle').hover();
            assert(await page.locator('#tooltip').evaluate(n => getComputedStyle(n).backgroundColor === getComputedStyle(document.querySelector('.panel')).backgroundColor));
            await page.keyboard.press('Escape');
            for (const [width, height] of [[1440,960], [375,812], [320,568], [812,375]]) {
                await page.setViewportSize({width,height});
                assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && document.documentElement.scrollHeight <= innerHeight), `${theme} ${width}: document must fit viewport`);
                const viewport = page.viewportSize();
                assert.equal(viewport.width, width);
                if (width === 1440 || width === 375) await page.screenshot({path: path.join(artifacts, `${theme}-${width}.png`)});
                await page.locator('#overview-tab').click();
                assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth&&document.documentElement.scrollHeight<=innerHeight),`home ${theme} ${width}: document must fit viewport`);
                if(width===1440||width===375)await page.screenshot({path:path.join(artifacts,`home-${theme}-${width}.png`)});
                await page.locator('#performance-tab').click();
            }
        }
        assert.deepEqual(errors, []); assert.deepEqual(network, []);
        console.log(`Offline design passed: computed filters, metric eligibility, targets, no-data, stable rows/selection, dialogs, keyboard, CSV, themes and responsive layout. Artifacts: ${artifacts}`);
    } finally {await browser.close();}
})().catch(error => {console.error(error); process.exitCode = 1;});
