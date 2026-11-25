let dashboardHealthFilter = 'all';
let dashboardCustomRange = {from: new Date(Date.now() - 3600000), to: new Date()};
let dashboardDetailOpen = false;

function homeDuration(value) {return value == null ? '—' : formatDuration(value);}
function homeSpeed(value) {return value == null ? '—' : Number(value).toFixed(1);}
function homeSuccess(row) {return row.request_count ? row.performance.success_count / row.request_count : null;}
function homeState(row) {
    if (row.performance.error_count) return {id:'errors', label:'有记录错误'};
    if (row.request_count < 20) return {id:'few', label:'样本不足'};
    return {id:'clear', label:'未见记录错误'};
}
function homeText(tag, className, value) {
    const node = document.createElement(tag);node.className=className || '';if(value !== undefined)node.textContent=value;return node;
}
function homeSet(node,value) {if(node.textContent!==String(value))node.textContent=value;}

function renderUsageBreakdowns(data) {
    for(const [id,rows]of [['operations-table-body',data.by_key],['traffic-table-body',data.by_provider]]) {
        const body=document.getElementById(id),existing=new Map(Array.from(body.children,node=>[node.dataset.group,node]));
        for(const [index,row]of rows.entries()) {
            let tr=existing.get(row.group_key);existing.delete(row.group_key);
            if(!tr){tr=homeText('tr');tr.dataset.group=row.group_key;for(let i=0;i<4;i++)tr.append(homeText('td'));}
            for(const [i,value]of [row.group_key,formatTokens(row.request_count),formatTokens(row.total_tokens),formatPercent(cacheRate(row.total_input_tokens,row.total_cached_tokens))].entries())homeSet(tr.children[i],value);
            if(body.children[index]!==tr)body.insertBefore(tr,body.children[index]||null);
        }
        for(const node of existing.values())node.remove();
        if(!rows.length){const tr=homeText('tr');tr.dataset.group='empty';const td=homeText('td','','暂无数据');td.colSpan=4;tr.append(td);body.replaceChildren(tr);}
    }
    for(const [id,rows]of [['protocol-distribution',data.by_protocol],['status-distribution',data.by_status_code]]) {
        const host=document.getElementById(id),existing=new Map(Array.from(host.children,node=>[node.dataset.group,node]));
        for(const [index,row]of rows.entries()) {
            let item=existing.get(row.group_key);existing.delete(row.group_key);
            if(!item){item=homeText('span');item.dataset.group=row.group_key;host.append(item);}
            homeSet(item,row.group_key+' '+formatTokens(row.request_count));
            if(host.children[index]!==item)host.insertBefore(item,host.children[index]||null);
        }
        for(const node of existing.values())node.remove();
        if(!rows.length){const item=homeText('span','','—');item.dataset.group='empty';host.replaceChildren(item);}
    }
}

function renderDashboardHome(data) {
    const p=data.observed,total=data.total,count=p.success_count+p.error_count;
    setMetric('metric-success-rate',count ? formatPercent(p.success_count/count) : '—');
    setMetric('analysis-errors','错误 '+formatTokens(p.error_count)+(p.stream_errors?' · 流内 '+p.stream_errors:''));
    setMetric('metric-first-byte',homeDuration(p.first_byte_p50_ms));
    setMetric('metric-first-byte-p95','P95 '+homeDuration(p.first_byte_p95_ms)+' · n='+formatTokens(p.first_byte_samples));
    setMetric('metric-tps',homeSpeed(p.estimated_tps_p50));
    setMetric('metric-tps-samples','P10 '+homeSpeed(p.estimated_tps_p10)+' · n='+formatTokens(p.speed_samples));
    const params=analyticsParams(document.getElementById('time-range').value),from=params.get('from'),to=params.get('to');
    const minutes=from&&to?(new Date(to)-new Date(from))/60000:null;
    const range=document.getElementById('time-range');
    for(const button of document.querySelectorAll('[data-window]'))button.setAttribute('aria-pressed',String(button.dataset.window===range.value));
    setMetric('metric-rpm',minutes>0?'RPM '+(total.request_count/minutes).toFixed(1):'全部历史');
    const groups=data.model_channels,host=document.getElementById('channel-state');
    const options=new Map(Array.from(host.options,node=>[node.value,node]));
    for(const state of [{id:'all',label:'全部'}, {id:'errors',label:'有错误'}, {id:'clear',label:'未见错误'}, {id:'few',label:'样本不足'}]) {
        let option=options.get(state.id);
        if(!option){option=document.createElement('option');option.value=state.id;host.append(option);}
        const size=state.id==='all'?groups.length:groups.filter(row=>homeState(row).id===state.id).length;
        homeSet(option,state.label+' '+size);
    }
    host.value=dashboardHealthFilter;
    document.getElementById('channel-clear').hidden=dashboardHealthFilter==='all';
    const order=document.getElementById('channel-sort').value;
    const rows=groups.filter(row=>dashboardHealthFilter==='all'||homeState(row).id===dashboardHealthFilter).slice();
    rows.sort((a,b)=>order==='errors'?b.performance.error_count-a.performance.error_count||b.request_count-a.request_count:order==='speed'?(b.performance.estimated_tps_p50??-1)-(a.performance.estimated_tps_p50??-1)||b.request_count-a.request_count:order==='first_byte'?(a.performance.first_byte_p50_ms??Infinity)-(b.performance.first_byte_p50_ms??Infinity)||b.request_count-a.request_count:b.request_count-a.request_count);
    setMetric('channel-count',rows.length+' 渠道');
    const body=document.getElementById('channel-table-body'),existing=new Map(Array.from(body.children,node=>[node.dataset.channel,node]));
    for(const [index,row]of rows.entries()) {
        const key=JSON.stringify([row.provider_name,row.model]);let tr=existing.get(key);existing.delete(key);
        if(!tr){tr=homeText('tr');tr.dataset.channel=key;tr.tabIndex=0;tr.setAttribute('aria-label',row.model+'，'+row.provider_name+'，查看性能详情');
            for(const label of ['模型 / 供应商','请求 / RPM','成功率 / 错误','首包 P50 / P95','估算 TPS · P50 / P10','耗时 P95 / 重试']){const td=homeText('td');td.dataset.label=label;td.append(homeText('strong'),homeText('small'));tr.append(td);}
            homeSet(tr.children[0].children[0],row.model);homeSet(tr.children[0].children[1],row.provider_name);
            tr.onclick=()=>{if(!window.getSelection()?.toString())openChannelDetail(tr.dataset.channel);};
            tr.onkeydown=event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();openChannelDetail(tr.dataset.channel);}};
        }
        const p=row.performance;
        const values=[null,[formatTokens(row.request_count),minutes>0?'RPM '+(row.request_count/minutes).toFixed(1):'全部历史'],
            [formatPercent(homeSuccess(row)),'错误 '+p.error_count+(p.stream_errors?' · 流内 '+p.stream_errors:'')],
            [homeDuration(p.first_byte_p50_ms),homeDuration(p.first_byte_p95_ms)+' · n='+formatTokens(p.first_byte_samples)],
            [homeSpeed(p.estimated_tps_p50),homeSpeed(p.estimated_tps_p10)+' · n='+formatTokens(p.speed_samples)],
            [homeDuration(p.latency_p95_ms),'重试 '+formatPercent(p.retried_requests/row.request_count)]];
        for(let i=1;i<values.length;i++){homeSet(tr.children[i].children[0],values[i][0]);homeSet(tr.children[i].children[1],values[i][1]);}
        tr.children[2].classList.toggle('channel-has-errors',p.error_count>0);
        if(body.children[index]!==tr)body.insertBefore(tr,body.children[index]||null);
    }
    for(const node of existing.values())node.remove();
    if(!rows.length){const tr=homeText('tr');tr.dataset.channel='empty';const td=homeText('td','','暂无数据');td.colSpan=6;tr.append(td);body.replaceChildren(tr);}
}

function openChannelDetail(key) {
    const row=analyticsData.model_channels.find(row=>JSON.stringify([row.provider_name,row.model])===key);if(!row)return;
    const p=row.performance,params=analyticsParams(document.getElementById('time-range').value);
    const fields=[['已结束请求',formatTokens(row.request_count)],['完整成功率',formatPercent(homeSuccess(row))],['记录错误',String(p.error_count)],['HTTP成功但流内错误',String(p.stream_errors)],['首包 P50 / P95',homeDuration(p.first_byte_p50_ms)+' / '+homeDuration(p.first_byte_p95_ms)],['首包样本',String(p.first_byte_samples)],['估算 TPS P50 / P10',homeSpeed(p.estimated_tps_p50)+' / '+homeSpeed(p.estimated_tps_p10)],['TPS 流式样本',String(p.speed_samples)],['成功请求耗时 P95',homeDuration(p.latency_p95_ms)],['有重试请求',String(p.retried_requests)],['已记录费用',formatCost(row.total_cost_cents)],['每次请求费用',formatCost(row.total_cost_cents/row.request_count)]];
    const shell=homeText('div','dialog-shell'),header=homeText('div','dialog-header'),heading=homeText('div','dialog-heading');
    const title=homeText('h2','dialog-title',row.model);title.id='dialog-title';
    heading.append(title,homeText('p','dialog-description',row.provider_name+' · 当前筛选快照'));
    const close=homeText('button','icon-button dialog-close');const icon=document.createElementNS('http://www.w3.org/2000/svg','svg');icon.setAttribute('viewBox','0 0 24 24');icon.setAttribute('class','icon');icon.setAttribute('aria-hidden','true');const line=document.createElementNS('http://www.w3.org/2000/svg','path');line.setAttribute('d','m6 6 12 12M6 18 18 6');icon.append(line);close.append(icon);close.type='button';close.setAttribute('aria-label','关闭性能详情');close.setAttribute('onclick','Modal.close()');header.append(heading,close);
    const content=homeText('div','dialog-body'),dl=homeText('dl','dashboard-detail-grid');
    for(const [name,value]of fields){const cell=homeText('div');cell.append(homeText('dt','',name),homeText('dd','',value));dl.append(cell);}content.append(dl);
    content.append(homeText('p','dashboard-detail-note','首包记录的是首个网络数据块，不代表首个有效 Token。TPS = 输出 Token /（总耗时−首包耗时），仅统计有明确流式标记、有效输出与时长的成功请求。缺失度量不填零。'));
    content.append(homeText('p','dashboard-detail-note','观测状态不是健康评分：有记录错误优先标出；少于20条记录显示样本不足；其余仅表示未见记录错误。请求状态结合流内错误判断，费用仅包含已记录的用量。'));
    const range=homeText('p','dashboard-detail-note','时段：'+(params.get('from')||'全部历史')+' 至 '+(params.get('to')||'现在'));content.append(range);shell.append(header,content);
    dashboardDetailOpen=true;Modal.open(shell.outerHTML,{drawer:true});
    const observer=new MutationObserver(()=>{if(document.getElementById('modal-backdrop').hidden){dashboardDetailOpen=false;observer.disconnect();}});observer.observe(document.getElementById('modal-backdrop'),{attributes:true,attributeFilter:['hidden']});
}

document.addEventListener('DOMContentLoaded',()=>{
    const localValue=date=>new Date(date.getTime()+8*3600000).toISOString().slice(0,16);
    document.getElementById('dashboard-from').value=localValue(dashboardCustomRange.from);document.getElementById('dashboard-to').value=localValue(dashboardCustomRange.to);
    document.getElementById('channel-state').onchange=event=>{dashboardHealthFilter=event.target.value;if(analyticsData)renderDashboardHome(analyticsData);};
    document.getElementById('channel-sort').onchange=()=>{if(analyticsData)renderDashboardHome(analyticsData);};
    for(const button of document.querySelectorAll('[data-window]'))button.onclick=()=>{const range=document.getElementById('time-range');range.value=button.dataset.window;range.dispatchEvent(new Event('change'));};
    document.getElementById('channel-clear').onclick=()=>{dashboardHealthFilter='all';renderDashboardHome(analyticsData);};
    document.getElementById('dashboard-custom-range').addEventListener('submit',event=>{
        event.preventDefault();const from=new Date(document.getElementById('dashboard-from').value+':00+08:00'),to=new Date(document.getElementById('dashboard-to').value+':00+08:00');
        const error=document.getElementById('dashboard-time-error');if(!Number.isFinite(+from)||!Number.isFinite(+to)||to<=from){error.textContent='结束时间必须晚于开始时间';return;}
        error.textContent='';dashboardCustomRange={from,to};loadAnalytics();
    });
});
