const AdminFilters = {
    controls: new Map(),
    async mount(definitions, onChange) {
        const response = await fetch('/v1/admin/filter-options', { credentials: 'include' });
        if (response.status === 401) return redirectToLogin();
        if (!response.ok) throw new Error('筛选项加载失败');
        const options = await response.json();
        this.currency = options.currency;
        for (const [id, dimension, label] of definitions) {
            const host = document.getElementById(id);
            if (!host) continue;
            const values = new Set();
            const details = document.createElement('details');
            details.className = 'multi-filter';
            const summary = document.createElement('summary');
            const title = document.createElement('span');
            const chevron = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            chevron.setAttribute('viewBox', '0 0 24 24'); chevron.setAttribute('class', 'icon');
            chevron.innerHTML = '<path d="m6 9 6 6 6-6"/>';
            summary.append(title, chevron);
            const panel = document.createElement('div'); panel.className = 'multi-filter-panel';
            const search = document.createElement('input');
            search.type = 'search'; search.placeholder = `搜索${label}`; search.setAttribute('aria-label', `搜索${label}`);
            const toolbar = document.createElement('div'); toolbar.className = 'multi-filter-tools';
            const all = document.createElement('button'); all.type = 'button'; all.textContent = '选择搜索结果';
            const clear = document.createElement('button'); clear.type = 'button'; clear.textContent = '清空';
            toolbar.append(all, clear);
            const list = document.createElement('div'); list.className = 'multi-filter-list';
            const update = () => {
                title.textContent = values.size ? `${label} · ${values.size}` : `全部${label}`;
                summary.classList.toggle('has-selection', values.size > 0);
                for (const input of list.querySelectorAll('input')) input.checked = values.has(input.value);
            };
            for (const item of options[dimension] || []) {
                const value = typeof item === 'string' ? item : item.value;
                const text = typeof item === 'string' ? item : item.label;
                const row = document.createElement('label'); row.title = text;
                const check = document.createElement('input'); check.type = 'checkbox'; check.value = value;
                const name = document.createElement('span'); name.textContent = text;
                row.append(check, name); list.append(row);
                check.addEventListener('change', () => {check.checked ? values.add(value) : values.delete(value); update(); onChange();});
            }
            if (!list.children.length) {const empty=document.createElement('p');empty.textContent='暂无本地配置';list.append(empty);}
            search.addEventListener('input', () => {
                const query = search.value.toLocaleLowerCase();
                for (const row of list.querySelectorAll('label')) row.hidden = !row.textContent.toLocaleLowerCase().includes(query);
            });
            all.onclick = () => {for(const row of list.querySelectorAll('label:not([hidden])')) values.add(row.querySelector('input').value);update();onChange();};
            clear.onclick = () => {values.clear();update();onChange();};
            panel.append(search, toolbar, list); details.append(summary, panel);
            host.replaceWith(details); details.id=id;
            const positionPanel = () => {
                if (!details.open) return;
                const anchor = summary.getBoundingClientRect();
                const margin = 12;
                const below = window.innerHeight - anchor.bottom - margin - 6;
                const above = anchor.top - margin - 6;
                const available = Math.max(above, below);
                panel.style.left = Math.max(margin, Math.min(anchor.left, window.innerWidth - panel.offsetWidth - margin)) + 'px';
                panel.style.maxHeight = Math.max(120, available) + 'px';
                panel.style.top = available < 120 ? margin + 'px' : below >= above ? anchor.bottom + 6 + 'px' : 'auto';
                panel.style.bottom = available >= 120 && above > below ? window.innerHeight - anchor.top + 6 + 'px' : 'auto';
            };
            details.addEventListener('toggle',()=>{
                if (!details.open) return;
                for(const other of document.querySelectorAll('.multi-filter[open]')) if(other!==details) other.open=false;
                positionPanel();
            });
            window.addEventListener('resize', positionPanel);
            document.addEventListener('scroll', event => { if (!panel.contains(event.target)) positionPanel(); }, true);
            this.controls.set(id, {values, update, dimension, available:new Set(Array.from(list.querySelectorAll('input')).map(e=>e.value))});
            update();
        }
    },
    append(params) {
        for (const {values,dimension} of this.controls.values()) if(values.size) params.set(dimension,JSON.stringify([...values]));
        return params;
    },
    restore(id, names) {
        const control=this.controls.get(id); if(!control)return;
        control.values.clear();
        for(const value of names)if(control.available.has(value))control.values.add(value);
        control.update();
    },
    clear() {for(const control of this.controls.values()){control.values.clear();control.update();}}
};
document.addEventListener('click',event=>{for(const menu of document.querySelectorAll('.multi-filter[open]'))if(!menu.contains(event.target))menu.open=false;});
document.addEventListener('keydown',event=>{if(event.key==='Escape')for(const menu of document.querySelectorAll('.multi-filter[open]')){menu.open=false;menu.querySelector('summary').focus();}});
