// Recent observations belong to this form instance, independent of editable configuration.
var providerRecentState;
function providerStatText(node, value) {
    const text = String(value);
    if (node.textContent !== text) node.textContent = text;
}
function providerStatDuration(value) {
    return value == null ? '—' : (value / 1000).toFixed(2) + ' s';
}
function providerStatSpeed(value) {
    return value == null ? '—' : Number(value).toFixed(1);
}
function renderProviderModelStats(row) {
    const state = providerRecentState;
    if (!state || !state.form.contains(row)) return;
    const host = row.querySelector('[data-model-stats]');
    const model = row.querySelector('[name$="[name]"]').value.trim();
    const channel = state.channels?.get(row.dataset.observedModel);
    const empty = !state.provider ? '保存供应商后可查看统计' : !row.dataset.observedModel || model !== row.dataset.observedModel ? '保存模型后可查看对应统计' : !state.channels ? '等待近期统计' : !channel ? '此时段没有已结束请求' : null;
    if (empty) {
        if (!host.querySelector('[data-stat-empty]')) {
            const message = document.createElement('span');message.dataset.statEmpty = '';host.replaceChildren(message);
        }
        providerStatText(host.firstElementChild, empty);
        return;
    }
    if (!host.querySelector('[data-stat-value]')) {
        host.replaceChildren();
        for (const label of ['请求 ', '成功率 ', '首包 P50 / P95 ', '估算 TPS P50 / P10 ']) {
            const field = document.createElement('span');
            field.append(document.createTextNode(label));
            const value = document.createElement('strong');value.dataset.statValue = '';field.append(value);
            field.append(document.createElement('small'));host.append(field);
        }
    }
    const p = channel.performance;
    const values = [
        [channel.request_count.toLocaleString(), channel.request_count < 20 ? ' · 样本不足' : ''],
        [(p.success_count / channel.request_count * 100).toFixed(1) + '%', ' · 错误 ' + p.error_count],
        [providerStatDuration(p.first_byte_p50_ms) + ' / ' + providerStatDuration(p.first_byte_p95_ms), ' · n=' + p.first_byte_samples],
        [providerStatSpeed(p.estimated_tps_p50) + ' / ' + providerStatSpeed(p.estimated_tps_p10), ' · n=' + p.speed_samples]
    ];
    for (const [index, [value, note]] of values.entries()) {
        providerStatText(host.children[index].querySelector('strong'), value);
        providerStatText(host.children[index].querySelector('small'), note);
    }
}
async function loadProviderRecentStats(changedWindow = false) {
    const state = providerRecentState;
    if (!state?.form.isConnected || !state.provider || (state.request && !changedWindow)) return;
    state.request?.abort();
    const controller = new AbortController();state.request = controller;
    const minutes = Number(document.getElementById('provider-recent-window').value);
    const to = new Date(), from = new Date(to.getTime() - minutes * 60000);
    const params = new URLSearchParams({from: from.toISOString(), to: to.toISOString(), bucket: 'minute', providers: JSON.stringify([state.provider])});
    const status = document.getElementById('provider-recent-status');
    const button = document.getElementById('provider-recent-refresh');
    if (changedWindow) {
        state.channels = null;
        state.form.querySelectorAll('[data-model-row]').forEach(renderProviderModelStats);
    }
    providerStatText(status, '正在更新…');button.disabled = true;
    try {
        const response = await fetch('/v1/admin/analytics/dashboard?' + params, {credentials: 'include', signal: controller.signal});
        if (response.status === 401) { redirectToLogin();return; }
        if (!response.ok) throw new Error('统计加载失败');
        const data = await response.json();
        if (providerRecentState !== state || state.request !== controller || !state.form.isConnected) return;
        state.channels = new Map(data.model_channels.filter(row => row.provider_name === state.provider).map(row => [row.model, row]));
        state.form.querySelectorAll('[data-model-row]').forEach(renderProviderModelStats);
        providerStatText(status, `最近 ${minutes} 分钟 · 更新于 ${to.toLocaleTimeString('zh-CN', {hour12: false})}`);
    } catch (error) {
        if (providerRecentState === state && state.request === controller && error.name !== 'AbortError') {
            providerStatText(status, state.channels ? '更新失败，保留上次统计，请重试' : '统计加载失败，请重试');
        }
    } finally {
        if (providerRecentState === state && state.request === controller) {state.request = null;button.disabled = false;}
    }
}
function initializeProviderRecentStats() {
    if (providerRecentState) {clearInterval(providerRecentState.timer);providerRecentState.request?.abort();}
    const form = document.querySelector('.provider-form');
    const state = {form, provider: form.dataset.statsProvider, channels: null, request: null};
    providerRecentState = state;
    state.timer = setInterval(() => {
        if (!form.isConnected) {clearInterval(state.timer);state.request?.abort();return;}
        if (!document.hidden && !document.getElementById('provider-models-panel').hidden && !state.request) loadProviderRecentStats();
    }, 30000);
    document.getElementById('provider-recent-refresh').disabled = !state.provider;
    document.getElementById('provider-recent-window').disabled = !state.provider;
}
