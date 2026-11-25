const LogDetail = { id: null, request: null, data: null, tab: 'preview', returnFocus: null };

function detailElement(tag, className, text) {
    const node = document.createElement(tag); node.className = className;
    if(text !== undefined) node.textContent = text;
    return node;
}
function detailButton(label, action) {
    const button = detailElement('button', 'outline-button', label); button.type = 'button';
    button.addEventListener('click', action); return button;
}
function detailStatus(log) {
    if(log.status === 'running') return log.retry_count ? '重试中' : '处理中';
    return ({success:'成功',failed:'失败',interrupted:'已中断'}[log.status] || log.status) + (log.status_code ? ` · ${log.status_code}` : '');
}
function detailParse(value) {
    if(typeof value !== 'string') return value;
    try {return JSON.parse(value);} catch {return value;}
}
function detailMeta(log) {
    const value = detailParse(log.meta);
    return value && typeof value === 'object' ? value : {};
}
async function openDetail(id) {
    const overlay = document.getElementById('detail-dialog');
    if(overlay.classList.contains('hidden')) LogDetail.returnFocus = document.activeElement;
    LogDetail.id = id; LogDetail.tab = 'preview'; LogDetail.data = null;
    overlay.classList.remove('hidden');document.body.classList.add('modal-open');
    clearTimeout(refreshTimer); refreshTimer = null;
    logsRequest?.abort();
    document.getElementById('log-dialog-title').textContent = '调用详情';
    document.getElementById('log-dialog-subtitle').textContent = '正在加载请求与响应';
    document.getElementById('detail-body').replaceChildren(detailElement('p','log-detail-empty','加载中…'));
    overlay.querySelector('[aria-label="关闭详情"]').focus();
    await refreshLogDetail();
}
async function refreshLogDetail() {
    if(!LogDetail.id) return;
    LogDetail.request?.abort();
    const request = new AbortController(); LogDetail.request = request;
    const body = document.getElementById('detail-body');body.setAttribute('aria-busy','true');
    const refresh = document.querySelector('[aria-label="刷新详情"]');refresh.disabled = true;
    try {
        const response = await fetch('/v1/admin/logs/' + encodeURIComponent(LogDetail.id), {credentials:'include',signal:request.signal});
        if(response.status === 401) {redirectToLogin();return;}
        if(!response.ok) throw new Error(response.status === 404 ? '这条调用记录已不存在' : '详情加载失败，请重试');
        const data = await response.json();
        if(LogDetail.request !== request || !LogDetail.id) return;
        LogDetail.data = data; renderDetail(data);
    } catch(error) {
        if(error.name !== 'AbortError' && LogDetail.request === request) {
            if(LogDetail.data) Toast.error(error.message);
            else body.replaceChildren(detailElement('p','log-detail-empty',error.message),detailButton('重新加载',refreshLogDetail));
        }
    } finally {
        if(LogDetail.request === request) {body.setAttribute('aria-busy','false');LogDetail.request = null;refresh.disabled = false;}
    }
}
function closeDetailDialog() {
    if(!LogDetail.id) return;
    LogDetail.id = null; LogDetail.request?.abort();LogDetail.request = null;LogDetail.data = null;
    document.getElementById('detail-dialog').classList.add('hidden');
    document.getElementById('detail-body').replaceChildren();document.body.classList.remove('modal-open');
    if(LogDetail.returnFocus?.isConnected) LogDetail.returnFocus.focus({preventScroll:true});
    scheduleRefresh();
}
function renderDetail(data) {
    const log = data.log || data, running = log.status === 'running';
    document.getElementById('log-dialog-title').textContent = log.model || '调用详情';
    document.getElementById('log-dialog-subtitle').textContent = `${log.provider_name || '未选择供应商'} · ${formatTime(log.created_at)}`;
    const summary = detailElement('div','log-detail-summary');
    const identity = detailElement('div','log-detail-identity');
    if(log.logo_url) {
        const image = document.createElement('img');image.src=log.logo_url;image.alt='';image.referrerPolicy='no-referrer';image.onerror=()=>image.remove();identity.append(image);
    }
    identity.append(detailElement('span',running ? 'badge-running' : log.status === 'success' ? 'badge-success' : 'badge-error',detailStatus(log)),
        detailElement('span','log-model-name',log.key_display_name || log.api_key_id),detailElement('span','log-detail-muted',`${log.protocol} · ${log.operation}`));
    const metrics = detailElement('dl','log-detail-metrics');
    const values = [['总耗时',formatDuration(log.latency_ms)],['首字耗时',log.first_byte_latency_ms ? formatDuration(log.first_byte_latency_ms) : '—'],
        ['输入 / 输出',running ? '—' : `${(log.input_tokens || 0).toLocaleString()} / ${(log.output_tokens || 0).toLocaleString()}`],
        ['缓存命中',running ? '—' : formatPercent(cacheRate(log.input_tokens,log.cached_tokens))],['输出速度',running ? '—' : formatTps(log.output_tokens,log.latency_ms,log.first_byte_latency_ms)+' Token/s'],['费用',running ? '—' : formatCost(log.cost_cents)]];
    for(const [label,value] of values) {const item=document.createElement('div');item.append(detailElement('dt','',label),detailElement('dd','',value));metrics.append(item);}
    summary.append(identity,metrics);
    const body=document.getElementById('detail-body');body.replaceChildren(summary);
    if(log.error_code || log.error) {
        const error=detailElement('div','log-detail-error');error.setAttribute('role','status');
        error.append(detailElement('strong','',log.error_code || '调用错误'),detailElement('span','',log.error || ''));body.append(error);
    }
    const tabs=detailElement('div','log-detail-tabs');tabs.setAttribute('role','tablist');tabs.setAttribute('aria-label','调用详情分类');
    for(const [id,label] of [['preview','内容预览'],['request','请求'],['response','响应'],['retry',`重试${log.retry_count ? ' · '+log.retry_count : ''}`],['metadata','元数据']]) {
        const tab=detailElement('button','',label);tab.type='button';tab.id='log-tab-'+id;tab.dataset.tab=id;tab.setAttribute('role','tab');tab.setAttribute('aria-controls','log-tab-content');
        tab.onclick=()=>selectDetailTab(id);tabs.append(tab);
    }
    tabs.addEventListener('keydown',event=>{
        if(!['ArrowRight','ArrowLeft','Home','End'].includes(event.key)) return;
        const items=Array.from(tabs.children),index=items.indexOf(document.activeElement);if(index<0)return;
        const next=event.key==='Home'?0:event.key==='End'?items.length-1:(index+(event.key==='ArrowRight'?1:-1)+items.length)%items.length;
        event.preventDefault();items[next].click();items[next].focus();
    });
    const panel=detailElement('div','log-detail-content');panel.id='log-tab-content';panel.tabIndex=0;panel.setAttribute('role','tabpanel');
    body.append(tabs,panel);selectDetailTab(LogDetail.tab);
}
function detailCopy(value,button) {
    const text=typeof value==='string'?value:JSON.stringify(value,null,2);
    const write=navigator.clipboard?.writeText(text);
    if(write) write.then(()=>Toast.success('已复制')).catch(()=>Toast.error('复制失败，请选择文字复制'));
    else { // Clipboard API is unavailable on plain HTTP LAN deployments.
        const previous=document.activeElement,input=document.createElement('textarea');input.value=text;input.style.cssText='position:fixed;opacity:0;pointer-events:none';
        document.getElementById('detail-dialog').append(input);input.select();
        let copied=false;try{copied=document.execCommand('copy');}catch{}
        input.remove();(previous || button)?.focus({preventScroll:true});(copied?Toast.success:Toast.error).call(Toast,copied?'已复制':'复制失败，请选择文字复制');
    }
}
function detailToolbar(title,value) {
    const bar=detailElement('div','log-content-toolbar');bar.append(detailElement('h3','',title));
    if(value!=null) {const button=detailButton('复制',()=>detailCopy(value,button));bar.append(button);}
    return bar;
}
function detailText(value,className='log-message-text',limit=16000) {
    const wrapper=document.createElement('div'),text=typeof value==='string'?value:JSON.stringify(value,null,2);
    const pre=detailElement('pre',className,text.slice(0,limit));wrapper.append(pre);
    if(text.length>limit) {const more=detailButton(`展开全部（${text.length.toLocaleString()} 字符）`,()=>{pre.textContent=text;more.remove();});more.classList.add('log-text-expand');wrapper.append(more);}
    return wrapper;
}
function renderRawBody(panel,value,title) {
    panel.append(detailToolbar(title,value));
    if(value==null) {panel.append(detailElement('p','log-detail-empty','正文未保存或已按保留策略清理，Token、耗时与费用记录仍保留。'));return;}
    const controls=detailElement('div','log-code-controls'),wrap=detailButton('关闭换行',()=>{
        const pre=panel.querySelector('.log-detail-code');const off=pre.classList.toggle('log-no-wrap');wrap.textContent=off?'开启换行':'关闭换行';
    });controls.append(wrap);panel.querySelector('.log-content-toolbar').append(controls);
    panel.append(detailText(value,'log-detail-code',120000));
}
function detailContent(value) {
    if(value==null) return '';
    if(typeof value==='string') return value;
    if(Array.isArray(value)) return value.map(detailContent).filter(Boolean).join('\n\n');
    if(typeof value.text==='string') return value.text;
    if(typeof value.content==='string' || Array.isArray(value.content)) return detailContent(value.content);
    if(value.type==='tool_use' || value.type==='function_call') return `${value.name || '工具调用'}\n${typeof value.arguments==='string'?value.arguments:JSON.stringify(value.input || value.arguments || {},null,2)}`;
    if(value.type==='tool_result' || value.type==='function_call_output') return detailContent(value.content || value.output);
    if(value.type==='thinking') return value.thinking || '';
    if(value.type==='image' || value.type==='image_url' || value.type==='input_image') return '[图片附件]';
    if(value.type==='input_file' || value.type==='file') return '[文件附件]';
    return JSON.stringify(value,null,2);
}
function detailMessages(value,side) {
    value=detailParse(value);
    if(value==null)return [];
    if(typeof value==='string') {
        if(side==='response' && value.includes('data:')) {
            let text='',reasoning='';const tools=new Map();
            for(const line of value.split(/\r?\n/)) {
                if(!line.startsWith('data:'))continue;
                try {
                    const event=JSON.parse(line.slice(5).trim()),delta=event.choices?.[0]?.delta || event.delta;
                    if(event.type==='response.output_text.delta')text+=event.delta || '';
                    else if(event.type==='content_block_delta') {text+=event.delta?.text || '';reasoning+=event.delta?.thinking || '';}
                    else if(delta && typeof delta==='object') {
                        text+=delta.content || delta.text || '';reasoning+=delta.reasoning_content || '';
                        for(const tool of delta.tool_calls || []) {const id=tool.index ?? tool.id ?? 0,old=tools.get(id)||{name:'',arguments:''};old.name+=tool.function?.name || '';old.arguments+=tool.function?.arguments || '';tools.set(id,old);}
                    }
                }catch{}
            }
            const messages=[];
            if(reasoning)messages.push({role:'思考',text:reasoning});
            if(text)messages.push({role:'assistant',text});
            for(const tool of tools.values())messages.push({role:'工具调用 · '+tool.name,text:tool.arguments});
            if(messages.length)return messages;
        }
        return [{role:side==='request'?'请求':'响应',text:value}];
    }
    const result=[];
    if(value.system)result.push({role:'system',text:detailContent(value.system)});
    if(value.instructions)result.push({role:'system',text:detailContent(value.instructions)});
    let messages=value.messages || (Array.isArray(value.input)?value.input:null) || value.output || value.choices?.map(choice=>choice.message || choice.delta);
    if(!messages && value.content)messages=[value];
    if(!messages && typeof value.input==='string')messages=[{role:'user',content:value.input}];
    for(const message of Array.isArray(messages)?messages:[]) {
        if(typeof message==='string'){result.push({role:'user',text:message});continue;}
        if(message.reasoning_content)result.push({role:'思考',text:message.reasoning_content});
        const text=detailContent(message.content ?? (message.type?message:null));
        if(text)result.push({role:message.role || message.type || 'assistant',text});
        for(const tool of message.tool_calls || [])result.push({role:'工具调用 · '+(tool.function?.name || tool.name || ''),text:tool.function?.arguments || JSON.stringify(tool,null,2)});
    }
    if(!result.length)result.push({role:side==='request'?'请求参数':'响应内容',text:JSON.stringify(value,null,2)});
    return result;
}
function renderConversation(panel,data) {
    for(const [key,title] of [['response','模型回复'],['request','请求消息']]) {
        const group=detailElement('section','log-message-group'),value=data[key+'_body'];
        const messages=detailMessages(value,key);
        group.append(detailToolbar(title,value==null?null:messages.map(message=>message.text).join('\n\n')));
        if(value==null) group.append(detailElement('p','log-detail-empty',key==='response' && (data.log || data).status==='running'?'请求仍在处理中，响应尚未完成。':'正文未保存或已清理，可在上方查看用量与状态。'));
        else for(const message of messages) {
            const row=detailElement('article','log-message');row.append(detailElement('div','log-message-head',message.role),detailText(message.text));group.append(row);
        }
        panel.append(group);
    }
}
function renderRetries(panel,log) {
    const retry=detailMeta(log).retry || {},attempts=Array.isArray(retry.attempts)?retry.attempts:[];
    panel.append(detailToolbar('重试过程'));
    if(!log.retry_count) {panel.append(detailElement('p','log-detail-empty','本次调用没有重试。'));return;}
    panel.append(detailElement('p','log-detail-muted',`重试 ${log.retry_count || 0} 次 · 总退避 ${formatDuration(retry.total_backoff_ms || 0)}`));
    for(const attempt of attempts) {
        const item=detailElement('div','log-retry-attempt');
        item.append(detailElement('strong','',`第 ${attempt.attempt} 次 · ${attempt.provider_name || '—'} · ${attempt.status_code || '未收到响应'}`),
            detailElement('p','log-detail-muted',`${attempt.protocol || ''} · 下次重试前等待 ${formatDuration(attempt.backoff_ms_before_next || 0)}`));
        if(attempt.error_code || attempt.error_message)item.append(detailElement('p','',`${attempt.error_code || ''} ${attempt.error_message || ''}`));panel.append(item);
    }
    if(!attempts.length)panel.append(detailElement('p','log-detail-empty','该记录未保存逐次尝试明细。'));
}
function selectDetailTab(id) {
    LogDetail.tab=id;const data=LogDetail.data;if(!data)return;
    const log=data.log || data,panel=document.getElementById('log-tab-content');panel.replaceChildren();panel.scrollTop=0;
    for(const button of document.querySelectorAll('.log-detail-tabs button')) {const selected=button.dataset.tab===id;button.setAttribute('aria-selected',String(selected));button.tabIndex=selected?0:-1;}
    panel.setAttribute('aria-labelledby','log-tab-'+id);
    if(id==='preview')renderConversation(panel,data);
    else if(id==='request' || id==='response')renderRawBody(panel,data[id+'_body'],id==='request'?'请求原文':'响应原文');
    else if(id==='retry')renderRetries(panel,log);
    else {
        const fields=detailElement('dl','log-detail-fields');
        for(const [name,value] of [['请求 ID',log.request_id],['日志 ID',log.id],['上游模型',log.model],['Key',log.key_display_name || log.api_key_id],['供应商',log.provider_name],['协议',log.protocol],['创建时间',formatTime(log.created_at)],['完成时间',log.finished_at?formatTime(log.finished_at):'尚未完成'],['总 Token',log.total_tokens ?? (log.input_tokens || 0)+(log.output_tokens || 0)],['缓存命中 Token',log.cached_tokens || 0],['缓存写入 Token',log.cache_write_tokens || 0]])fields.append(detailElement('dt','',name),detailElement('dd','',value ?? '—'));
        panel.append(fields);renderRawBody(panel,detailMeta(log),'元数据 JSON');
    }
}
document.addEventListener('keydown',event=>{
    const overlay=document.getElementById('detail-dialog');if(!LogDetail.id || overlay.classList.contains('hidden'))return;
    if(event.key==='Escape') {event.preventDefault();closeDetailDialog();return;}
    if(event.key==='Tab') {
        const controls=Array.from(overlay.querySelectorAll('button:not(:disabled),[tabindex="0"]')).filter(node=>node.getClientRects().length),first=controls[0],last=controls.at(-1);
        if(event.shiftKey && document.activeElement===first){event.preventDefault();last?.focus();}
        else if(!event.shiftKey && document.activeElement===last){event.preventDefault();first?.focus();}
    }
});
