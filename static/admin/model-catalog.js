const catalogState = { models: [], providers: [], enabled: false, editing: null, saving: false };
const SOURCE_ROW_HEIGHT = 72;
const sourceState = { catalog: null, index: [], filtered: [], loading: false, saving: null, selected: new Map(), provider: null };

async function catalogRequest(url, options = {}) {
    const response = await fetch(url, { credentials: 'include', ...options });
    if (response.status === 401) {
        redirectToLogin();
        throw new Error('登录已过期');
    }
    const data = await response.json();
    if (!response.ok) throw new Error(data.error?.message || data.message || '请求失败');
    return data;
}

async function loadPlatformModels() {
    try {
        const [catalog, providers] = await Promise.all([
            catalogRequest('/v1/admin/platform-models'),
            catalogRequest('/v1/admin/providers')
        ]);
        catalogState.models = catalog.models;
        catalogState.enabled = catalog.enabled;
        catalogState.providers = providers;
        document.getElementById('catalog-enabled').checked = catalog.enabled;
        renderPlatformModels();
    } catch (error) {
        Toast.error(error.message);
        document.getElementById('catalog-list').textContent = '模型目录加载失败';
    }
}

function bindingCount(name) {
    return catalogState.providers.reduce((count, provider) => count +
        provider.models.filter(model => model.status === 'enabled' &&
            (model.platform_model || model.name) === name).length, 0);
}

function textElement(tag, className, value) {
    const element = document.createElement(tag);
    element.className = className;
    element.textContent = value;
    return element;
}

function providerLogo(providerId, providerName) {
    const box = document.createElement('span');
    box.className = 'catalog-logo';
    const fallback = textElement('span', 'catalog-logo-fallback', providerName.slice(0, 1).toUpperCase());
    box.append(fallback);
    if (/^[a-z0-9-]+$/i.test(providerId)) {
        const img = document.createElement('img');
        img.alt = '';
        img.loading = 'lazy';
        img.onerror = () => img.remove();
        img.src = `/v1/admin/platform-models/source/logos/${encodeURIComponent(providerId)}`;
        box.prepend(img);
    }
    return box;
}

function renderPlatformModels() {
    const list = document.getElementById('catalog-list');
    document.getElementById('catalog-count').textContent = `${catalogState.models.length} 个模型`;
    if (!catalogState.models.length) {
        list.replaceChildren(textElement('div', 'catalog-empty', '尚未配置平台模型'));
        return;
    }
    const table = list.querySelector('table') || document.createElement('table');
    const previous = new Map(Array.from(table.querySelectorAll('tbody tr')).map(row=>[row.dataset.modelName,row]));
    const body = table.querySelector('tbody') || document.createElement('tbody');
    let cursor = body.firstElementChild;
    table.className = 'catalog-table';
    const head = document.createElement('thead');
    head.innerHTML = '<tr><th scope="col">模型</th><th scope="col">公开名称</th><th scope="col">路由与供应商</th><th scope="col">上下文</th><th scope="col">操作</th></tr>';
    if(!table.querySelector('thead')) table.append(head);
    catalogState.models.forEach((model, index) => {
        const row = document.createElement('tr');
        row.dataset.modelName = model.name;
        row.dataset.rowAction = 'openModelByName';
        row.dataset.rowValue = model.name;
        row.tabIndex = 0;
        row.setAttribute('aria-label', `查看模型 ${model.name}`);
        const identity = document.createElement('td');
        const identityText = document.createElement('div');
        const name = textElement('span', 'catalog-name', model.display_name || model.name);
        if (model.status !== 'enabled') name.append(textElement('span', 'catalog-status', '已停用'));
        identityText.append(name, textElement('span', 'catalog-id', model.name));
        const identityGroup = document.createElement('div');
        identityGroup.className = 'catalog-model-identity';
        identityGroup.append(modelLogo(model.logo_url, model.display_name || model.name));
        identityGroup.append(identityText);
        identity.append(identityGroup);
        const names = document.createElement('td');
        names.append(textElement('span', '', model.aliases.length ? model.aliases.join(', ') : model.name),
            textElement('span', 'catalog-meta', model.aliases.length ? '仅公开别名' : '公开本名'));
        const route = document.createElement('td');
        route.append(textElement('span', '', model.route_to ? `转到 ${model.route_to}` : '按本模型路由'),
            textElement('span', 'catalog-meta', `${bindingCount(model.route_to || model.name)} 个供应商模型绑定`));
        const source = document.createElement('td');
        source.append(textElement('span', 'catalog-meta', model.context_window ? model.context_window.toLocaleString() + ' Token' : '—'));
        const actions = document.createElement('td');
        actions.className = 'catalog-actions';
        const edit = document.createElement('button');
        edit.className = 'icon-button';
        edit.title = '编辑模型';
        edit.setAttribute('aria-label', `编辑 ${model.name}`);
        edit.innerHTML = '<svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="m16 3 5 5-12 12-6 1 1-6L16 3ZM14 5l5 5"/></svg>';
        edit.type = 'button';
        edit.addEventListener('click', () => openModelEditor(catalogState.models.findIndex(item=>item.name===model.name)));
        const remove = document.createElement('button');
        remove.className = 'icon-button catalog-action-delete';
        remove.title = '删除模型';
        remove.setAttribute('aria-label', `删除 ${model.name}`);
        remove.innerHTML = '<svg class="icon" aria-hidden="true" viewBox="0 0 24 24"><path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/></svg>';
        remove.type = 'button';
        remove.addEventListener('click', () => deletePlatformModel(catalogState.models.findIndex(item=>item.name===model.name)));
        const toggle = document.createElement('button');
        toggle.type = 'button'; toggle.className = 'catalog-inline-status';
        toggle.textContent = model.status === 'enabled' ? '启用' : '停用';
        toggle.setAttribute('aria-label', `${model.status === 'enabled' ? '停用' : '启用'}模型 ${model.name}`);
        toggle.addEventListener('click',()=>togglePlatformModel(catalogState.models.findIndex(item=>item.name===model.name),toggle));
        actions.append(toggle, edit, remove);
        row.append(identity, names, route, source, actions);
        const existing = previous.get(model.name);
        if(existing) {
            // Keep row actions and their focus when metadata changes.
            for(let i=0;i<4;i++) {
                const cell=existing.children[i];
                if(cell.innerHTML!==row.children[i].innerHTML) cell.replaceChildren(...Array.from(row.children[i].childNodes));
            }
            const button=existing.querySelector('.catalog-inline-status');
            button.textContent=toggle.textContent;button.setAttribute('aria-label',toggle.getAttribute('aria-label'));
            previous.delete(model.name);
        }
        const target=existing || row;
        if(target!==cursor) body.insertBefore(target,cursor);
        cursor=target.nextElementSibling;
    });
    for(const row of previous.values())row.remove();
    if(!body.isConnected)table.append(body);
    if(!table.isConnected)list.replaceChildren(table);
}

function openModelByName(name) {
    const index = catalogState.models.findIndex(model => model.name === name);
    if(index >= 0) openModelEditor(index);
}

function modelLogo(url, name) {
    const box = textElement('span', 'catalog-logo', name.slice(0, 1).toUpperCase());
    if(url) {
        const image = document.createElement('img');
        image.alt = ''; image.referrerPolicy = 'no-referrer'; image.loading = 'lazy';
        image.onerror = () => image.remove(); image.src = url; box.append(image);
    }
    return box;
}

function openModelEditor(index = null) {
    catalogState.editing = index;
    Modal.open(document.getElementById('catalog-editor-template').innerHTML, {drawer:true});
    const form = document.getElementById('catalog-editor');
    const current = index === null ? null : catalogState.models[index];
    form.elements.name.value = current?.name || '';
    form.elements.name.readOnly = index !== null;
    form.elements.display_name.value = current?.display_name || '';
    form.elements.aliases.value = current?.aliases?.join(', ') || '';
    form.elements.description.value = current?.description || '';
    form.elements.context_window.value = current?.context_window || '';
    form.elements.logo_url.value = current?.logo_url || '';
    const preview = document.getElementById('catalog-logo-preview');
    const showLogo = () => preview.replaceChildren(modelLogo(form.elements.logo_url.value.trim(), form.elements.display_name.value || form.elements.name.value || 'M'));
    form.elements.logo_url.addEventListener('input', showLogo); showLogo();
    form.elements.input_price.value = current?.pricing?.input_per_1k ?? '';
    form.elements.output_price.value = current?.pricing?.output_per_1k ?? '';
    form.elements.enabled.checked = current ? current.status === 'enabled' : true;
    const route = form.elements.route_to;
    catalogState.models.filter(model => model.name !== current?.name && !model.route_to)
        .forEach(model => route.add(new Option(model.name, model.name)));
    route.value = current?.route_to || '';
    document.getElementById('dialog-title').textContent = current ? '编辑模型' : '新增模型';
    form.elements.name.focus();
    let dirty = false;
    form.addEventListener('input',()=>dirty=true);
    form.addEventListener('change',()=>dirty=true);
    Modal.beforeClose=()=>!form.querySelector('[type=submit]').disabled && (!dirty || confirm('放弃未保存的模型修改？'));
}


async function persistCatalog(models, enabled, render = true) {
    if(catalogState.saving) throw new Error('正在保存模型配置，请稍后再试');
    catalogState.saving=true;
    try {
        await catalogRequest('/v1/admin/platform-models', {
            method: 'PUT', headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ models, enabled })
        });
        catalogState.models = models;
        catalogState.enabled = enabled;
        document.getElementById('catalog-enabled').checked = enabled;
        if(render) renderPlatformModels();
    } finally {catalogState.saving=false;}
}

async function savePlatformModel(event) {
    event.preventDefault();
    const form = event.target;
    const errorBox = document.getElementById('catalog-form-error');
    errorBox.hidden = true;
    const name = form.elements.name.value.trim();
    const aliases = form.elements.aliases.value.split(',').map(value => value.trim()).filter(Boolean);
    const input = form.elements.input_price.value;
    const output = form.elements.output_price.value;
    const existing = catalogState.editing === null ? null : catalogState.models[catalogState.editing];
    const model = {
        name,
        display_name: form.elements.display_name.value.trim(),
        description: form.elements.description.value.trim(),
        aliases,
        route_to: form.elements.route_to.value || null,
        context_window: form.elements.context_window.value ? Number(form.elements.context_window.value) : null,
        pricing: input !== '' || output !== '' ? { input_per_1k: Number(input || 0), output_per_1k: Number(output || 0) } : null,
        logo_url: form.elements.logo_url.value.trim() || null,
        source: existing?.source || null,
        source_updated_at: existing?.source_updated_at || null,
        status: form.elements.enabled.checked ? 'enabled' : 'disabled'
    };
    const models = catalogState.models.slice();
    if (catalogState.editing === null) models.push(model);
    else models[catalogState.editing] = model;
    const button = form.querySelector('[type="submit"]');
    button.disabled = true;
    try {
        await persistCatalog(models, catalogState.enabled);
        Modal.close(true);
        Toast.success('模型配置已保存');
    } catch (error) {
        errorBox.textContent = error.message;
        errorBox.hidden = false;
    } finally {
        button.disabled = false;
    }
}

async function togglePlatformModel(index, button) {
    if(catalogState.saving || button.getAttribute('aria-busy')==='true')return;
    const before = catalogState.models[index];
    const models = catalogState.models.slice();
    const enabled = before.status !== 'enabled';
    models[index] = {...before,status:enabled?'enabled':'disabled'};
    button.setAttribute('aria-busy','true');
    try {
        await persistCatalog(models,catalogState.enabled,false);
        button.textContent=enabled?'启用':'停用';
        button.setAttribute('aria-label',`${enabled?'停用':'启用'}模型 ${before.name}`);
        const name=button.closest('tr').querySelector('.catalog-name');
        name.querySelector('.catalog-status')?.remove();
        if(!enabled)name.append(textElement('span','catalog-status','已停用'));
        Toast.success(enabled?'模型已启用':'模型已停用');
    } catch(error){Toast.error(error.message);}
    finally {button.setAttribute('aria-busy','false');}
}

async function deletePlatformModel(index) {
    const model = catalogState.models[index];
    if (!confirm(`删除平台模型 ${model.name}？`)) return;
    try {
        await persistCatalog(catalogState.models.filter((_, i) => i !== index), catalogState.enabled);
        Toast.success('模型已删除');
    } catch (error) {
        Toast.error(error.message);
    }
}

async function toggleCatalog(checkbox) {
    const enabled = checkbox.checked;
    if (enabled && !confirm('启用后，只公开已匹配或手动绑定的模型；未匹配模型将不再对外提供。继续吗？')) {
        checkbox.checked = catalogState.enabled;
        return;
    }
    checkbox.disabled = true;
    try {
        await persistCatalog(catalogState.models, enabled);
        Toast.success(enabled ? '模型目录已启用' : '模型目录已停用');
    } catch (error) {
        checkbox.checked = catalogState.enabled;
        Toast.error(error.message);
    } finally {
        checkbox.disabled = false;
    }
}

function showSourceMessage(message) {
    const summary = document.getElementById('source-summary');
    const items = document.getElementById('source-items');
    if (!summary || !items) return;
    summary.textContent = message;
    items.style.height = '100%';
    items.replaceChildren(textElement('div', 'catalog-picker-state', message));
}

function sourceSearchMatches(haystack, query) {
    return query.split(/\s+/).every(term => haystack.includes(term));
}

function filterSourceCatalog() {
    if (!sourceState.catalog) return;
    const providerQuery = document.getElementById('source-provider-filter').value.trim().toLocaleLowerCase();
    for(const button of document.querySelectorAll('#source-provider-list button[data-provider]')) {
        button.hidden=!!button.dataset.provider && !sourceSearchMatches(button.dataset.search,providerQuery);
        button.classList.toggle('is-active',(button.dataset.provider||null)===sourceState.provider);
    }
    const modelQuery = document.getElementById('source-model-filter').value.trim().toLocaleLowerCase();
    sourceState.filtered = sourceState.index.filter(model =>
        (!sourceState.provider || model.provider === sourceState.provider) && sourceSearchMatches(model.providerSearch, providerQuery) &&
        sourceSearchMatches(model.modelSearch, modelQuery));
    const viewport = document.getElementById('source-viewport');
    viewport.scrollTop = 0;
    document.getElementById('source-summary').textContent = `${sourceState.filtered.length.toLocaleString()} / ${sourceState.index.length.toLocaleString()} 个模型`;
    updateSourcePickedCount();
    renderSourceRows();
}

function updateSourcePickedCount() {
    const label = document.getElementById('source-picked-count');
    if (label) label.textContent = `已选 ${sourceState.selected.size}`;
    const button=document.getElementById('source-import-selected');
    if(button){button.textContent=`导入所选 (${sourceState.selected.size})`;button.disabled=!sourceState.selected.size||!!sourceState.saving;}
}

function createSourceRow(model, index) {
    const row = document.createElement('button');
    row.type = 'button';
    row.className = 'catalog-source-item';
    row.style.top = `${index * SOURCE_ROW_HEIGHT}px`;
    row.disabled = sourceState.saving !== null;
    row.title = `${model.name} · ${model.id}`;
    row.setAttribute('aria-label', `选择 ${model.name}，${model.providerName}`);
    row.setAttribute('aria-pressed',String(sourceState.selected.has(model.id)));
    row.append(providerLogo(model.provider, model.providerName));
    const details = document.createElement('span');
    details.className = 'catalog-source-details';
    details.append(textElement('strong', '', model.name),
        textElement('span', 'catalog-source-id', model.id),
        textElement('span', 'catalog-source-meta', `${model.providerName}${model.context_window ? ` · ${model.context_window.toLocaleString()} 上下文` : ''}${model.last_updated ? ` · ${model.last_updated}` : ''}`));
    const action = textElement('span', 'catalog-source-action',
        sourceState.saving === `${model.provider}/${model.id}` ? '保存中' :
            sourceState.selected.has(model.id) ? '已选择' : catalogState.models.some(item => item.name === model.id) ? '已存在' : '选择');
    row.append(details, action);
    row.addEventListener('click', () => {
        if(sourceState.selected.has(model.id)) sourceState.selected.delete(model.id);
        else sourceState.selected.set(model.id,model);
        row.setAttribute('aria-pressed',String(sourceState.selected.has(model.id)));
        action.textContent=sourceState.selected.has(model.id)?'已选择':catalogState.models.some(item=>item.name===model.id)?'已存在':'选择';
        updateSourcePickedCount();
    });
    return row;
}

function renderSourceRows() {
    const viewport = document.getElementById('source-viewport');
    const items = document.getElementById('source-items');
    if (!viewport || !items || !sourceState.catalog) return;
    const total = sourceState.filtered.length;
    if (!total) {
        items.style.height = '100%';
        items.replaceChildren(textElement('div', 'catalog-picker-state', '没有匹配的模型'));
        return;
    }
    const start = Math.max(0, Math.floor(viewport.scrollTop / SOURCE_ROW_HEIGHT) - 4);
    const end = Math.min(total, start + Math.ceil(viewport.clientHeight / SOURCE_ROW_HEIGHT) + 9);
    items.style.height = `${total * SOURCE_ROW_HEIGHT}px`;
    const rows = [];
    for (let index = start; index < end; index++) {
        rows.push(createSourceRow(sourceState.filtered[index], index));
    }
    items.replaceChildren(...rows);
}

async function refreshSourceCatalog() {
    if (sourceState.loading) return;
    sourceState.loading = true;
    const button = document.querySelector('.catalog-picker-actions .outline-button');
    if (button) button.disabled = true;
    showSourceMessage('正在加载 models.dev 目录...');
    try {
        const catalog = await catalogRequest('/v1/admin/platform-models/source');
        const providerNames = new Map(catalog.providers.map(provider => [provider.id, provider.name]));
        sourceState.catalog = catalog;
        sourceState.index = catalog.models.map(model => {
            const providerName = providerNames.get(model.provider) || model.provider;
            return {
                ...model,
                providerName,
                providerSearch: `${model.provider} ${providerName}`.toLocaleLowerCase(),
                modelSearch: `${model.id} ${model.name}`.toLocaleLowerCase()
            };
        });
        if (document.getElementById('source-viewport')) {renderSourceProviders();filterSourceCatalog();}
    } catch (error) {
        showSourceMessage(error.message);
        Toast.error(error.message);
    } finally {
        sourceState.loading = false;
        if (button?.isConnected) button.disabled = false;
    }
}

function openSourcePicker() {
    Modal.open(document.getElementById('catalog-picker-template').innerHTML);
    if (sourceState.catalog) {renderSourceProviders();filterSourceCatalog();}
    else refreshSourceCatalog();
    document.getElementById('source-model-filter').focus();
}

function sourceModelForImport(source, models) {
    const index = models.findIndex(model => model.name === source.id);
    const existing = index >= 0 ? models[index] : null;
    const model = existing ? {
        ...existing,
        display_name: existing.display_name || source.name,
        description: existing.description || source.description,
        context_window: source.context_window ?? existing.context_window ?? null,
        logo_url: existing.logo_url || `https://models.dev/logos/${encodeURIComponent(source.provider)}.svg`,
        source: `models.dev/${source.provider}`,
        source_updated_at: source.last_updated || existing.source_updated_at || null
    } : {
        name: source.id,
        display_name: source.name,
        description: source.description,
        aliases: [],
        route_to: null,
        context_window: source.context_window,
        logo_url: `https://models.dev/logos/${encodeURIComponent(source.provider)}.svg`,
        pricing: null,
        source: `models.dev/${source.provider}`,
        source_updated_at: source.last_updated,
        status: 'enabled'
    };
    return model;
}

function renderSourceProviders() {
    const list=document.getElementById('source-provider-list');
    list.replaceChildren();
    for(const provider of [{id:'',name:'全部供应商',model_count:sourceState.index.length},...sourceState.catalog.providers]) {
        const button=document.createElement('button');button.type='button';button.dataset.provider=provider.id;
        button.dataset.search=`${provider.id} ${provider.name}`.toLocaleLowerCase();
        const label=textElement('span','',provider.name);const count=textElement('span','catalog-meta',provider.model_count.toLocaleString());
        button.append(label,count);
        button.onclick=()=>{sourceState.provider=provider.id||null;filterSourceCatalog();};list.append(button);
    }
}
function selectFilteredSources(){for(const model of sourceState.filtered)sourceState.selected.set(model.id,model);renderSourceRows();updateSourcePickedCount();}
function clearSourceSelection(){sourceState.selected.clear();renderSourceRows();updateSourcePickedCount();}
async function importSelectedSources() {
    if(sourceState.saving||!sourceState.selected.size)return;
    const selected=[...sourceState.selected.values()];
    const models=catalogState.models.slice();
    for(const source of selected){const model=sourceModelForImport(source,models);const index=models.findIndex(item=>item.name===model.name);if(index<0)models.push(model);else models[index]=model;}
    sourceState.saving='batch';
    Modal.beforeClose=()=>!sourceState.saving;
    updateSourcePickedCount();renderSourceRows();
    try {await persistCatalog(models,catalogState.enabled);sourceState.selected.clear();Toast.success(`已导入 / 更新 ${selected.length} 个模型`);}
    catch(error){Toast.error(error.message);}
    finally {sourceState.saving=null;updateSourcePickedCount();renderSourceRows();}
}

document.addEventListener('DOMContentLoaded', loadPlatformModels);
