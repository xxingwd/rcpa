(() => {
    const interactive = 'a,button,input,select,textarea,label,summary,code,pre,[contenteditable],[data-row-ignore]';
    function activate(event) {
        if(event.defaultPrevented || (event.type === 'click' && (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey))) return;
        const row = event.target.closest('[data-row-action]');
        if(!row || event.target.closest(interactive) || window.getSelection()?.toString()) return;
        if(event.type === 'keydown' && (event.target !== row || event.key !== 'Enter' || event.isComposing)) return;
        if(row.dataset.rowAction === 'toggleProviderModelRow') {
            const button = row.querySelector('.provider-model-expand');
            if(button) {event.preventDefault();button.click();}
            return;
        }
        const action = row.dataset.rowAction;
        if(!['openDetail','openEditModal','openModelByName'].includes(action) || typeof window[action] !== 'function') return;
        event.preventDefault();
        row.focus({preventScroll:true});
        window[action](row.dataset.rowValue);
    }
    document.addEventListener('click', activate);
    document.addEventListener('keydown', activate);
})();
