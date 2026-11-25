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
        await page.goto(base+'/dashboard');
        await page.waitForFunction(()=>document.getElementById('metric-total-requests').textContent==='3');
        await page.locator('#refresh-interval').selectOption('0');
        assert.equal(await page.locator('.dashboard-kpis > div').count(),5);
        assert.equal(await page.locator('.dashboard-board details').count(),0,'All analysis blocks must be visible without expansion');
        for(const selector of ['#tokenTrendChart','#requestTrendChart','#latencyTrendChart','#operations-title','#traffic-title','#protocol-distribution','#status-distribution'])assert(await page.locator(selector).isVisible(),'Visible analysis: '+selector);
        const metrics=await api('GET','/v1/admin/analytics/dashboard?bucket=minute&from='+encodeURIComponent(new Date(Date.now()-3600000).toISOString()));
        assert.equal(await page.locator('#operations-table-body tr').count(),metrics.by_key.length);
        assert.equal(await page.locator('#traffic-table-body tr').count(),metrics.by_provider.length);
        assert((await page.locator('#protocol-distribution').innerText()).includes('completions 3'),'Protocol distribution must be visible without switching tabs');
        assert((await page.locator('#status-distribution').innerText()).includes('200 3'),'Status distribution must be visible without switching tabs');
        const usageRow=await page.locator('#operations-table-body tr').first().elementHandle();
        await page.evaluate(()=>loadAnalytics());
        assert(await usageRow.evaluate(node=>node.isConnected),'Usage refresh must preserve rows and selection');
        for(const row of metrics.model_channels) {
            const cells=page.locator('#channel-table-body tr').filter({hasText:row.model}).locator('td');
            assert.equal(await cells.nth(1).locator('strong').textContent(),String(row.request_count));
            assert.equal(await cells.nth(2).locator('strong').textContent(),'100.0%');
            assert((await page.locator('.dashboard-channel-table thead th').nth(3).textContent()).includes('P95'),'First-byte tail latency must have a visible column label');
            assert((await cells.nth(3).locator('small').textContent()).includes('n='+row.performance.first_byte_samples),'First-byte samples must be visible without opening details');
            assert.equal(await cells.nth(4).locator('strong').textContent(),'—','Non-streaming history has no estimated TPS');
            assert((await cells.nth(4).textContent()).includes('n=0'),'Missing speed sample count must be visible');
            assert((await cells.nth(5).textContent()).includes('重试 0.0%'),'Retry rate must be visible without opening details');
        }
        const shortRequest=page.waitForRequest(request=>request.url().includes('/v1/admin/analytics/dashboard?')&&request.method()==='GET');
        await page.getByRole('button',{name:'近 5 分钟',exact:true}).click();
        const shortUrl=new URL((await shortRequest).url());
        assert(Math.abs((new Date(shortUrl.searchParams.get('to'))-new Date(shortUrl.searchParams.get('from')))-300000)<1000,'Short window must query five minutes of stored data');
        await page.waitForFunction(()=>document.querySelector('[data-window="5m"]').getAttribute('aria-pressed')==='true');
        assert.equal(await page.locator('#time-range').inputValue(),'5m','Shortcuts and global time selection must stay synchronized');
        await page.selectOption('#channel-state','errors');
        assert.equal(await page.locator('#channel-table-body tr[data-channel=empty]').count(),1,'Status filtering must show an explicit empty state');
        await page.selectOption('#channel-state','few');
        assert.equal(await page.locator('#channel-table-body tr').count(),3,'Small samples must remain available without being labelled healthy');
        await page.selectOption('#channel-state','all');
        for(const [width,height] of [[1440,960],[1366,768],[1280,720],[1024,812]]) {
            await page.setViewportSize({width,height});
            await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
            assert(await page.locator('.page-body').evaluate(node=>node.scrollHeight<=node.clientHeight+1),'Dashboard overview must fit one screen: '+width+'x'+height);
            for(const selector of ['.dashboard-channel-card','.dashboard-trends','.dashboard-breakdowns'])assert(await page.locator(selector).evaluate(node=>{const box=node.getBoundingClientRect(),body=document.querySelector('.page-body').getBoundingClientRect();return box.top>=body.top&&box.bottom<=body.bottom+1;}),'All blocks must fit in view: '+selector+' '+width+'x'+height);
            const table=page.locator('.dashboard-channel-card .dashboard-table-scroll');
            await table.evaluate(node=>{const filler=document.createElement('div');filler.id='test-channel-overflow';filler.style.height='1200px';node.append(filler);});
            assert(await table.evaluate(node=>node.scrollHeight>node.clientHeight),'Long channel lists must scroll inside the table');
            assert(await page.locator('.page-body').evaluate(node=>node.scrollHeight<=node.clientHeight+1),'Long channel lists must not extend the overview');
            await page.locator('#test-channel-overflow').evaluate(node=>node.remove());
        }
        await page.setViewportSize({width:1440,height:960});
        assert.equal(await page.locator('#metric-tps').textContent(),'—','Non-streaming calls must not produce generation TPS');
        const refreshButton=page.getByRole('button',{name:'刷新仪表盘',exact:true});
        const tooltip=page.locator('#rcpa-tooltip');
        for(const theme of ['light','dark']) {
            await page.evaluate(theme=>ThemeManager.set(theme),theme);
            await refreshButton.hover();await tooltip.waitFor({state:'visible'});
            assert.equal(await tooltip.textContent(),'刷新仪表盘');
            assert.equal(await refreshButton.getAttribute('title'),null,'The native tooltip must not overlap the themed tooltip');
            const tooltipColors=await tooltip.evaluate(node=>{
                const actual=getComputedStyle(node),probe=document.createElement('div');
                probe.style.cssText='position:fixed;background:var(--card);color:var(--card-foreground);transition:none!important';
                document.body.append(probe);
                const background=getComputedStyle(probe).backgroundColor,foreground=getComputedStyle(probe).color;probe.remove();
                return {actual:[actual.backgroundColor,actual.color],expected:[background,foreground]};
            });
            assert.deepEqual(tooltipColors.actual,tooltipColors.expected,'Icon tooltip colors must follow '+theme);
            await page.screenshot({path:path.join(directory,'tooltip-'+theme+'.png')});
            for(const id of ['tokenTrendChart','requestTrendChart','latencyTrendChart']) {
                await page.locator('#'+id).scrollIntoViewIfNeeded();
                const point=await page.locator('#'+id).evaluate(canvas=>{
                    const chart=Chart.getChart(canvas),element=chart.getDatasetMeta(0).data[0],box=canvas.getBoundingClientRect();
                    return {x:box.x+element.x,y:box.y+element.y+Math.min(4,(element.base-element.y||0)/2)};
                });
                await page.mouse.move(point.x,point.y);
                await page.waitForFunction(id=>Chart.getChart(document.getElementById(id)).tooltip.opacity>0,id);
                assert(await page.locator('#'+id).evaluate(canvas=>{
                    const chart=Chart.getChart(canvas),styles=getComputedStyle(document.documentElement),options=chart.tooltip.options;
                    return options.backgroundColor===styles.getPropertyValue('--card').trim()&&options.titleColor===styles.getPropertyValue('--card-foreground').trim()&&options.bodyColor===options.titleColor&&options.borderColor===styles.getPropertyValue('--border').trim()&&options.borderWidth===1;
                }),'Chart tooltip colors must follow '+theme+': '+id);
            }
        }
        await page.evaluate(()=>ThemeManager.set('light'));
        await refreshButton.hover();await tooltip.waitFor({state:'visible'});
        await page.evaluate(()=>ThemeManager.set('dark'));
        assert.equal(await tooltip.evaluate(node=>getComputedStyle(node).backgroundColor),'rgb(28, 28, 31)','An open tooltip must update without another hover');
        await page.keyboard.press('Escape');await tooltip.waitFor({state:'hidden'});
        await page.mouse.move(0,0);await page.locator('#refresh-interval').focus();await page.keyboard.press('Tab');
        assert(await refreshButton.evaluate(node=>document.activeElement===node),'Tab from refresh interval must focus the refresh button');
        await tooltip.waitFor({state:'visible'});
        assert((await refreshButton.getAttribute('aria-describedby')).split(' ').includes('rcpa-tooltip'));
        await page.keyboard.press('Escape');await tooltip.waitFor({state:'hidden'});
        assert(await refreshButton.evaluate(node=>document.activeElement===node),'Dismissing a tooltip must keep keyboard focus');
        await page.evaluate(()=>ThemeManager.set('light'));
        await choose('dashboard-models',['public-a','public-b']);
        await page.waitForFunction(()=>document.getElementById('metric-total-requests').textContent==='2');
        await choose('dashboard-providers',['provider-b']);
        await page.waitForFunction(()=>document.getElementById('metric-total-requests').textContent==='1');
        assert.equal(await page.locator('#dashboard-models input[value=historical-public]').count(),0);
        assert.equal(await page.locator('canvas').count(),3);
        for (const id of ['latencyTrendChart']) {
            assert(await page.locator('#'+id).evaluate(canvas=>Chart.getChart(canvas).data.datasets.every(dataset=>dataset.pointRadius>0)),'Single-bucket trends must have visible points');
        }
        // Search input and checkbox nodes survive refreshes and stay focused.
        const search=await page.locator('#dashboard-providers input[type=search]').elementHandle();
        await search.fill('provider'); await search.focus();
        await page.evaluate(()=>loadAnalytics());
        assert(await search.evaluate(node=>node.isConnected&&document.activeElement===node));
        await page.locator('#dashboard-providers summary').click();
        const channelRow=page.locator('#channel-table-body tr[data-channel]').first();
        await channelRow.focus();await page.keyboard.press('Enter');await page.locator('.dashboard-detail-grid').waitFor();
        await page.keyboard.press('Escape');assert(await channelRow.evaluate(n=>document.activeElement===n));
        const channelNode=await channelRow.elementHandle();await page.evaluate(()=>loadAnalytics());assert(await channelNode.evaluate(n=>n.isConnected));
        await page.locator('#time-range').selectOption('custom');await page.locator('#dashboard-custom-range').waitFor({state:'visible'});
        await page.locator('#dashboard-from').fill('2026-01-02T12:00');await page.locator('#dashboard-to').fill('2026-01-01T12:00');
        await page.locator('#dashboard-custom-range button[type=submit]').click();assert(await page.locator('#dashboard-time-error').textContent());
        await page.locator('#time-range').selectOption('today');
        await page.screenshot({path:path.join(directory,'dashboard.png')});
        await page.goto(base+'/logs'); await page.locator('#filter-model summary').waitFor();
        await page.locator('#filter-refresh').selectOption('0');
        await choose('filter-model',['public-a','public-b']);
        await page.waitForFunction(()=>document.getElementById('logs-info').textContent.includes('共 2 条'));
        await choose('filter-provider',['provider-a']);
        await page.waitForFunction(()=>document.getElementById('logs-info').textContent.includes('共 1 条'));
        await page.goto(base+'/providers'); await page.locator('.provider-list-table').waitFor();
        assert.equal(await page.locator('.provider-list-url').count(),2);
        assert.equal(await page.locator('.provider-list-table .model-badge').count(),0);
        assert(!(await page.locator('.provider-list-table').innerText()).includes('messages'));
        await page.getByRole('button',{name:'编辑供应商 provider-a',exact:true}).hover();
        await tooltip.waitFor({state:'visible'});assert.equal(await tooltip.textContent(),'编辑供应商');
        await page.getByRole('button',{name:'编辑供应商 provider-a',exact:true}).click();
        await page.waitForURL('**/providers/provider-a/settings');
        const input=page.locator('[name=api_key]'); await input.fill('draft-secret');
        const inputNode=await input.elementHandle();
        await page.locator('[data-provider-section=models]').click();
        await page.locator('[data-provider-section=basic]').click();
        assert.equal(await input.inputValue(),'draft-secret');assert(await inputNode.evaluate(node=>node.isConnected));
        await page.locator('[data-provider-section=models]').click();
        await page.getByRole('button',{name:'获取上游模型'}).click();
        await page.locator('#upstream-preview:not([hidden])').waitFor();
        await page.locator('#upstream-search').fill('new');
        await page.locator('#upstream-preview').getByRole('button',{name:'选择搜索结果'}).click();
        await page.locator('#import-upstream').click();
        assert.equal(await page.locator('#models-container [data-model-row]').count(),3);
        await page.locator('button[type=submit]').click();
        await page.waitForURL('**/providers');
        const providers=await api('GET','/v1/admin/providers');
        assert.equal(providers.find(provider=>provider.name==='provider-a').models.length,3);
        await page.goto(base+'/model-catalog'); await page.locator('#catalog-count').waitFor();
        const opener=page.getByRole('button',{name:'新增模型'});await opener.click();
        await page.locator('.drawer-overlay').waitFor();
        await page.locator('#catalog-editor [name=name]').fill('local-model');
        page.once('dialog',dialog=>dialog.dismiss());await page.keyboard.press('Escape');
        assert.equal(await page.locator('#catalog-editor [name=name]').inputValue(),'local-model');
        await page.locator('#catalog-editor [type=submit]').click();
        await page.locator('#modal-backdrop').waitFor({state:'hidden'});
        assert(await opener.evaluate(node=>document.activeElement===node));
        const toggle=page.getByRole('button',{name:'停用模型 local-model',exact:true});const toggleNode=await toggle.elementHandle();
        await toggle.click();await page.getByRole('button',{name:'启用模型 local-model',exact:true}).waitFor();
        assert(await toggleNode.evaluate(node=>node.isConnected&&document.activeElement===node));
        await page.route('**/v1/admin/platform-models/source',route=>route.fulfill({json:{providers:[{id:'openai',name:'OpenAI',model_count:2},{id:'anthropic',name:'Anthropic',model_count:1}],models:[{id:'import-a',name:'Import A',provider:'openai',description:'',context_window:128000,last_updated:null},{id:'import-b',name:'Import B',provider:'openai',description:'',context_window:128000,last_updated:null},{id:'import-c',name:'Import C',provider:'anthropic',description:'',context_window:null,last_updated:null}]}}));
        await page.route('https://models.dev/logos/*',route=>route.fulfill({contentType:'image/svg+xml',body:'<svg xmlns="http://www.w3.org/2000/svg"/>'}));
        await page.route('**/v1/admin/platform-models/source/logos/*',route=>route.fulfill({contentType:'image/svg+xml',body:'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24"/></svg>'}));
        await page.getByRole('button',{name:'从 models.dev 导入',exact:true}).click();
        await page.locator('#source-provider-list button[data-provider=openai]').click();
        await page.getByRole('button',{name:'选择筛选结果',exact:true}).click();
        await page.locator('#source-import-selected').click();
        await page.waitForFunction(()=>document.getElementById('source-import-selected').textContent==='导入所选 (0)');
        const catalog=await api('GET','/v1/admin/platform-models');
        assert.deepEqual(catalog.models.map(model=>model.name),['local-model','import-a','import-b']);
        await page.keyboard.press('Escape');

        // Imported metadata becomes normal editable configuration; no source column or import counters.
        assert.equal(await page.locator('.catalog-table th').filter({hasText:'资料'}).count(),0);
        const logoURL='https://fixture.example/color-logo.svg';
        await page.route('https://fixture.example/color-logo.svg',route=>route.fulfill({contentType:'image/svg+xml',body:'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="5" fill="#10a37f"/><circle cx="12" cy="12" r="5" fill="#fbbf24"/></svg>'}));
        const modelRow=page.locator('[data-model-name="local-model"]');
        await modelRow.locator('td').nth(3).click();
        await page.locator('#catalog-editor [name=logo_url]').fill(logoURL);
        await page.locator('#catalog-editor [type=submit]').click();
        await page.locator('#modal-backdrop').waitFor({state:'hidden'});
        assert(await modelRow.evaluate(node=>document.activeElement===node),'Row opener focus returns after model save');
        const savedCatalog=await api('GET','/v1/admin/platform-models');
        assert.equal(savedCatalog.models.find(model=>model.name==='local-model').logo_url,logoURL);
        savedCatalog.models.push({name:'vendor-a',aliases:[],status:'enabled',logo_url:logoURL});
        await api('PUT','/v1/admin/platform-models',savedCatalog);
        await page.goto(base+'/providers');
        await page.locator('[data-provider-order-id="provider-a"] td').nth(2).click();
        await page.waitForURL('**/providers/provider-a/settings');
        await page.locator('[data-provider-section=models]').click();
        const providerModel=page.locator('[data-model-row]').first();
        await providerModel.click({position:{x:3,y:3}});
        assert.equal(await providerModel.locator('.provider-model-details').isVisible(),true,'Blank model area expands settings');
        await page.goto(base+'/keys');
        const keyRow=page.locator('[data-key-order-id]').first();
        await keyRow.locator('td').nth(7).click();
        await page.locator('#modal-body form').waitFor();
        await page.keyboard.press('Escape');
        assert(await keyRow.evaluate(node=>document.activeElement===node),'Key detail restores row focus');
        await keyRow.focus();await page.keyboard.press('Enter');
        await page.locator('#modal-body form').waitFor();
        await page.keyboard.press('Escape');
        await page.locator('#modal-backdrop').waitFor({state:'hidden'});
        assert(await keyRow.evaluate(node=>document.activeElement===node),'Keyboard-opened detail closes with one Escape even with a focused tooltip');
        await page.goto(base+'/logs');
        await page.locator('#filter-refresh').selectOption('0');
        await page.locator('#filter-model summary').waitFor();
        const logRow=page.locator('#logs-tbody [data-row-action]').filter({hasText:'vendor-a'}).first();
        await logRow.waitFor();
        assert.equal(await logRow.locator('img').getAttribute('src'),logoURL,'Log row uses configured direct logo URL');
        await logRow.locator('td').nth(6).click();
        await page.locator('.log-detail-tabs').waitFor();
        assert(await page.locator('#log-tab-content').textContent().then(text=>text.includes('hello')&&text.includes('fixture response')),'Preview extracts request and response messages');
        const requestTab=page.getByRole('tab',{name:'请求',exact:true});
        await requestTab.click();
        assert(await page.locator('.log-detail-code').textContent().then(text=>text.includes('messages')));
        await page.getByRole('tab',{name:'响应',exact:true}).click();
        assert(await page.locator('.log-detail-code').textContent().then(text=>text.includes('fixture response')));
        await page.getByRole('tab',{name:'重试',exact:true}).click();
        assert(await page.locator('#log-tab-content').textContent().then(text=>text.includes('没有重试')));
        await page.getByRole('tab',{name:'元数据',exact:true}).click();
        assert(await page.locator('#log-tab-content').textContent().then(text=>text.includes('请求 ID')));
        await page.getByRole('tab',{name:'内容预览',exact:true}).click();
        const content=page.locator('#log-tab-content');
        await content.evaluate(node=>{const p=document.createElement('div');p.style.cssText='height:2000px;min-height:2000px';node.append(p);});
        const box=await content.boundingBox();await page.mouse.move(box.x+4,box.y+4);await page.mouse.wheel(0,500);
        await page.waitForFunction(()=>document.getElementById('log-tab-content').scrollTop>0);
        assert.equal(await page.evaluate(()=>window.scrollY),0,'Detail scroll stays inside the panel');
        await page.keyboard.press('Escape');
        assert(await logRow.evaluate(node=>document.activeElement===node),'Log details restore focus');
        // Late responses may not reopen a closed panel or replace another selected record.
        const logs=await api('GET','/v1/admin/logs?limit=20');const firstID=logs.items[0].id,secondID=logs.items[1].id;
        let releaseDetail,detailStarted;
        const started=new Promise(resolve=>detailStarted=resolve),hold=new Promise(resolve=>releaseDetail=resolve);
        await page.route('**/v1/admin/logs/'+encodeURIComponent(firstID),async route=>{const response=await route.fetch();detailStarted();await hold;await route.fulfill({response}).catch(()=>{});});
        await page.evaluate(id=>{openDetail(id);},firstID);
        await started;
        await page.evaluate(id=>{openDetail(id);},secondID);
        await page.waitForFunction(id=>LogDetail.data?.id===id,secondID);
        releaseDetail();
        assert.equal(await page.evaluate(()=>LogDetail.data.id),secondID);
        await page.keyboard.press('Escape');
        await page.unroute('**/v1/admin/logs/'+encodeURIComponent(firstID));
        const previewCases=await page.evaluate(()=>({
            stream:detailMessages('data: {"choices":[{"delta":{"content":"streamed text"}}]}\n\ndata: [DONE]\n','response'),
            responses:detailMessages({output:[{role:'assistant',content:[{type:'output_text',text:'Responses reply'}]}]},'response'),
            messages:detailMessages({content:[{type:'text',text:'Messages reply'}]},'response'),
            tools:detailMessages({choices:[{message:{role:'assistant',content:null,tool_calls:[{function:{name:'search',arguments:'{"query":"hi"}'}}]}}]},'response')
        }));
        assert.equal(previewCases.stream[0].text,'streamed text');assert.equal(previewCases.responses[0].text,'Responses reply');assert.equal(previewCases.messages[0].text,'Messages reply');assert(previewCases.tools[0].role.includes('search'));
        for(const theme of ['light','dark'])for(const [width,height]of [[1440,960],[375,812],[320,568]]) {
            await page.setViewportSize({width,height});await page.evaluate(theme=>ThemeManager.set(theme),theme);
            await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
            await page.evaluate(id=>{openDetail(id);},secondID);await page.locator('.log-detail-tabs').waitFor();
            assert.equal(await page.locator('.log-detail-panel').evaluate(node=>node.scrollWidth>node.clientWidth),false,'Log detail horizontal overflow '+width+' '+theme);
            assert.equal(await page.evaluate(()=>document.documentElement.scrollHeight>innerHeight),false,'Log detail document overflow '+JSON.stringify(await page.evaluate(()=>({width:innerWidth,height:innerHeight,root:document.documentElement.scrollHeight,body:document.body.scrollHeight,scrollY,children:Array.from(document.body.children).filter(n=>n.getClientRects().length).map(n=>({tag:n.tagName,id:n.id,top:n.getBoundingClientRect().top,bottom:n.getBoundingClientRect().bottom,position:getComputedStyle(n).position}))}))));
            await page.screenshot({path:path.join(directory,'log-detail-'+width+'-'+theme+'.png')});
            await page.keyboard.press('Escape');
        }

        for(const theme of ['light','dark']) {
            for(const [width,height] of [[1440,960],[1024,812],[900,812],[640,812],[639,812],[375,812],[320,568],[812,375]]) {
                await page.setViewportSize({width,height});
                for(const route of ['/dashboard','/logs','/providers','/providers/provider-a/settings','/model-catalog','/keys','/config']) {
                    await page.goto(base+route);await page.evaluate(theme=>ThemeManager.set(theme),theme);
                    await overflow(`${route} ${width} ${theme}`);
                    const layout = await page.evaluate(() => {
                        const main = document.querySelector('.admin-main');
                        const body = document.querySelector('.page-body').getBoundingClientRect();
                        const style = getComputedStyle(main);
                        return {
                            documentHeight: document.documentElement.scrollHeight,
                            mainOverflow: main.scrollHeight - main.clientHeight,
                            contentBottom: main.getBoundingClientRect().bottom - parseFloat(style.paddingBottom),
                            bodyBottom: body.bottom,
                            bodyHeight: body.height,
                        };
                    });
                    const label = `${route} ${width}x${height} ${theme}`;
                    assert(layout.documentHeight <= height + 1, 'Document must not scroll: '+label);
                    assert(layout.mainOverflow <= 1, 'Main shell must not scroll: '+label);
                    assert(Math.abs(layout.contentBottom-layout.bodyBottom) <= 1, 'Content must fill remaining height: '+label);
                    assert(layout.bodyHeight > 0, 'Content must remain visible: '+label);
                    if(theme === 'light' && [1440,375,812].includes(width)) {
                        const selector = {'/dashboard':'.dashboard-page .page-body','/logs':'.logs-table-wrap','/providers':'#providers-list','/providers/provider-a/settings':'.provider-form-main','/model-catalog':'#catalog-list','/keys':'.data-table-scroll','/config':'.CodeMirror-scroll'}[route];
                        await page.locator(selector).waitFor();
                        const headerTop = await page.locator('.page-header').evaluate(node=>node.getBoundingClientRect().top);
                        if(route !== '/config') {
                            await page.locator(selector).evaluate(node=>{
                                const spacer = document.createElement('div');
                                spacer.style.cssText='height:2000px;min-height:2000px;flex-shrink:0';
                                spacer.setAttribute('aria-hidden','true');
                                node.append(spacer);
                            });
                        }
                        const region=page.locator(selector);
                        // Commit the newly overflowing region to Chromium's scroll tree before input.
                        await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
                        const bounds=await region.boundingBox();
                        const wheelTarget=await page.evaluate(({x,y})=>{const node=document.elementFromPoint(x+2,y+2);return node?.outerHTML.slice(0,180);},bounds);
                        await page.evaluate(()=>{
                            window.testWheelEvents=[];
                            document.addEventListener('wheel',event=>window.testWheelEvents.push({target:event.target.outerHTML.slice(0,120),x:event.clientX,y:event.clientY,prevented:event.defaultPrevented,delta:event.deltaY}),{passive:true});
                        });
                        // Target the region's edge so a nested table/chart does not capture the wheel.
                        await page.mouse.move(bounds.x+2,bounds.y+2);await page.mouse.wheel(0,500);
                        await page.waitForFunction(selector=>document.querySelector(selector).scrollTop>0,selector,{timeout:3000}).catch(async error=>{
                            const size=await region.evaluate(node=>({height:node.clientHeight,scrollHeight:node.scrollHeight,top:node.scrollTop,overflow:getComputedStyle(node).overflowY}));
                            await page.screenshot({path:path.join(directory,'scroll-failure.png')});
                            const events=await page.evaluate(()=>window.testWheelEvents);
                            throw new Error('Inner wheel scrolling failed: '+label+' '+selector+' '+JSON.stringify(size)+' Target: '+wheelTarget+' Events: '+JSON.stringify(events)+' Fixture: '+directory,{cause:error});
                        });
                        assert.equal(await page.locator('.page-header').evaluate(node=>node.getBoundingClientRect().top),headerTop,'Header must stay fixed: '+label);
                        assert.equal(await page.evaluate(()=>window.scrollY),0,'Document must stay fixed: '+label);
                        // Reaching the inner edge must not transfer scrolling to the shell.
                        await region.evaluate(node=>node.scrollTop=node.scrollHeight);
                        await page.mouse.wheel(0,500);
                        assert.equal(await page.evaluate(()=>window.scrollY),0,'No scroll chaining: '+label);
                        await region.evaluate(node=>node.scrollTop=0);
                    }
                    if(route==='/model-catalog') {
                        await page.getByRole('button',{name:'新增模型'}).click();
                        assert.equal(await page.locator('.modal-content').evaluate(node=>node.scrollWidth>node.clientWidth),false);
                        await page.keyboard.press('Escape');
                    }
                }
            }
        }
        assert.deepEqual(errors,[]);
        console.log('Admin browser regression passed. Screenshots and fixture log:',directory);
    } finally {
        if(browser)await browser.close();
        if(gateway&&gateway.exitCode===null){gateway.kill('SIGINT');await new Promise(resolve=>gateway.once('exit',resolve));}
        await close(upstream);fs.closeSync(log);
    }
})().catch(error=>{console.error(error);process.exitCode=1;});
