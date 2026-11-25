// Run after `cargo build`; uses a temporary database and an in-process upstream.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawn} = require('node:child_process');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const listen = server => new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve(server.address().port)));
const close = server => new Promise(resolve => server.close(resolve));
(async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'rcpa-admin-browser-'));
    const log = fs.openSync(path.join(directory, 'server.log'), 'a');
    let gateway, browser;
    const upstream = http.createServer(async (request, response) => {
        let body = ''; for await (const chunk of request) body += chunk;
        response.setHeader('content-type', 'application/json');
        if (request.url.endsWith('/models')) return response.end(JSON.stringify({data:['vendor-a','vendor-b','new-model','another-new'].map(id=>({id}))}));
        const model = JSON.parse(body).model;
        response.end(JSON.stringify({id:'fixture',object:'chat.completion',created:1,model,choices:[{index:0,message:{role:'assistant',content:'fixture response'},finish_reason:'stop'}],usage:{prompt_tokens:100,completion_tokens:20,total_tokens:120}}));
    });
    try {
        const upstreamPort = await listen(upstream);
        const reservation = http.createServer(); const port = await listen(reservation); await close(reservation);
        const base = `http://127.0.0.1:${port}`;
        gateway = spawn(path.resolve('target/debug/rcpa'), ['--data-dir',directory,'--token','browser-fixture-token','--port',String(port)], {stdio:['ignore',log,log]});
        const deadline = Date.now() + 30000;
        while (true) {
            if (gateway.exitCode !== null) throw new Error(fs.readFileSync(path.join(directory,'server.log'),'utf8'));
            try {if ((await fetch(base+'/health')).ok) break;} catch {}
            if (Date.now()>deadline) throw new Error('Fixture startup timed out');
            await new Promise(resolve=>setTimeout(resolve,100));
        }
        browser = await chromium.launch({headless:true,...(process.env.RCPA_BROWSER_PATH?{executablePath:process.env.RCPA_BROWSER_PATH}:{})});
        const context = await browser.newContext({viewport:{width:1440,height:960},reducedMotion:'reduce'});
        const page = await context.newPage(); const errors=[];
        page.on('pageerror',error=>errors.push(error.message));
        const api=async(method,url,data)=>{
            const response=await context.request.fetch(base+url,{method,data,headers:{'x-admin-token':'browser-fixture-token'}});
            assert(response.ok(),await response.text()); return response.json();
        };
        await api('POST','/v1/admin/login',{token:'browser-fixture-token'});
        for(const [name,model,alias] of [['provider-a','vendor-a','public-a'],['provider-b','vendor-b','public-b'],['retired','historical','historical-public']]) {
            await api('POST','/v1/admin/providers',{name,api_key:'fixture',models:[{name:model,aliases:[alias],status:'enabled',platform_model:null,pricing:{input_per_1k:1,output_per_1k:2}}],endpoints:[{protocol:'completions',base_url:`http://127.0.0.1:${upstreamPort}/v1/chat/completions`},{protocol:'messages',base_url:`http://127.0.0.1:${upstreamPort}/v1/messages`}],headers:{},status:'enabled',priority:0,model_passthrough:false});
        }
        const key=await api('POST','/v1/admin/keys',{name:'Browser Key'});
        for(const model of ['public-a','public-b','historical-public']) {
            const response=await context.request.post(base+'/v1/chat/completions',{headers:{'x-api-key':key.key},data:{model,messages:[{role:'user',content:'hello'}]}});
            assert(response.ok(),await response.text());
        }
        await api('DELETE','/v1/admin/providers/retired');
        const options=await api('GET','/v1/admin/filter-options');
        assert.deepEqual(options.models,['public-a','public-b']);
        const overflow=async label=>assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,label);
        const choose=async(id,values)=>{
            await page.locator('#'+id+' summary').click();
            for(const value of values)await page.locator('#'+id+' input[type=checkbox]').filter({visible:true}).evaluateAll((nodes,target)=>nodes.find(node=>node.value===target).click(),value);
        };
        page.setDefaultTimeout(10000);
        await page.goto(base+'/providers/provider-a/settings');
        await page.locator('[data-provider-section=models]').click();
        await page.waitForFunction(() => document.getElementById('provider-recent-status').textContent.includes('更新于'));
        assert.equal(await page.locator('#provider-recent-window').inputValue(), '15');
        const row = page.locator('[data-model-row]').first();
        const stats = row.locator('[data-model-stats]');
        assert((await stats.innerText()).includes('100.0%'));
        assert((await stats.innerText()).includes('— / —'));
        assert.equal(await stats.locator('strong').first().textContent(), '1', 'Only this supplier model is counted');
        assert.equal(await page.evaluate(() => providerFormIsDirty()), false);
        // Refresh never replaces model inputs or loses a draft and focus.
        await row.locator('.provider-model-expand').click();
        const price = row.locator('[name$="[input_price]"]');
        await price.fill('3.14');
        await page.evaluate(() => {
            window.originalModelRow = document.querySelector('[data-model-row]');
            window.originalStats = originalModelRow.querySelector('[data-model-stats] strong');
        });
        await page.evaluate(() => loadProviderRecentStats());
        assert(await page.evaluate(() => originalModelRow === document.querySelector('[data-model-row]') && originalStats === originalModelRow.querySelector('[data-model-stats] strong')));
        assert.equal(await price.inputValue(), '3.14');
        assert(await price.evaluate(node => document.activeElement === node));
        // Input changes must not attach saved model observations to another name.
        const modelInput = row.locator('[name$="[name]"]');
        await modelInput.fill('unsaved-model');
        assert((await stats.innerText()).includes('保存模型'));
        await modelInput.fill('vendor-a');
        assert((await stats.innerText()).includes('100.0%'));
        let release, started;
        const held = new Promise(resolve => release = resolve);
        const intercepted = new Promise(resolve => started = resolve);
        const handler = async route => {
            started(); await held;
            try {await route.fulfill({json: {model_channels: []}});} catch {}
        };
        await page.route('**/v1/admin/analytics/dashboard?*', handler);
        await page.selectOption('#provider-recent-window', '1440');await intercepted;
        await page.unroute('**/v1/admin/analytics/dashboard?*', handler);
        const requestPromise = page.waitForRequest(request => request.url().includes('/v1/admin/analytics/dashboard?'));
        await page.selectOption('#provider-recent-window', '5');
        const url = new URL((await requestPromise).url());
        assert.deepEqual(JSON.parse(url.searchParams.get('providers')), ['provider-a']);
        assert.equal(new Date(url.searchParams.get('to')) - new Date(url.searchParams.get('from')), 300000);
        await page.waitForFunction(() => document.getElementById('provider-recent-status').textContent.startsWith('最近 5 分钟'));
        release();
        assert.equal(await stats.locator('strong').first().textContent(), '1');
        // A range selection is not a config change; reset the deliberately edited price.
        await price.fill('1');
        assert.equal(await page.evaluate(() => providerFormIsDirty()), false);
        for (const theme of ['light','dark']) {
            await page.evaluate(theme => ThemeManager.set(theme), theme);
            for (const width of [1440, 390]) {
                await page.setViewportSize({width, height: 900});
                await overflow(theme + ' model stats ' + width);
                await page.locator('#provider-recent-window').scrollIntoViewIfNeeded();
                assert(await page.locator('#provider-recent-window').isVisible());
                await page.screenshot({path: path.join(directory, `recent-${theme}-${width}.png`)});
            }
        }
        await page.route('**/v1/admin/analytics/dashboard?*', route => route.fulfill({json: {model_channels: []}}));
        await page.evaluate(() => loadProviderRecentStats());
        assert((await stats.innerText()).includes('没有已结束请求'));
        await page.unroute('**/v1/admin/analytics/dashboard?*');
        await page.route('**/v1/admin/analytics/dashboard?*', route => route.fulfill({status: 500, json: {}}));
        await page.evaluate(() => loadProviderRecentStats());
        assert((await page.locator('#provider-recent-status').innerText()).includes('更新失败'));
        assert.deepEqual(errors, []);
        console.log('Provider model recent statistics regression passed:', directory);
    } finally {
        if(browser)await browser.close();
        if(gateway&&gateway.exitCode===null){gateway.kill('SIGINT');await new Promise(resolve=>gateway.once('exit',resolve));}
        await close(upstream);fs.closeSync(log);
    }
})().catch(error=>{console.error(error);process.exitCode=1;});
