(() => {
    const tooltip = document.createElement('div');
    tooltip.id = 'rcpa-tooltip';
    tooltip.className = 'admin-tooltip';
    tooltip.setAttribute('role', 'tooltip');
    tooltip.hidden = true;
    let anchor = null;
    let timer;
    const observer = new MutationObserver(() => {
        if (anchor && (!anchor.isConnected || !anchor.getClientRects().length)) hide();
    });

    function hide() {
        clearTimeout(timer);
        observer.disconnect();
        if (anchor) {
            const ids = (anchor.getAttribute('aria-describedby') || '').split(/\s+/).filter(id => id && id !== tooltip.id);
            if (ids.length) anchor.setAttribute('aria-describedby', ids.join(' '));
            else anchor.removeAttribute('aria-describedby');
        }
        anchor = null;
        tooltip.hidden = true;
    }

    function show(target, immediate = false) {
        if (!target || target.matches('input, select, textarea')) return;
        const text = target.getAttribute('title') || target.dataset.tooltip;
        if (!text) return;
        // Keep validation titles on inputs; replace display titles so native tooltips cannot overlap.
        target.dataset.tooltip = text;
        target.removeAttribute('title');
        if (!target.hasAttribute('aria-label') && target.matches('button, a') && !target.textContent.trim()) {
            target.setAttribute('aria-label', text);
        }
        hide();
        anchor = target;
        const reveal = () => {
            if (!target.isConnected || !target.getClientRects().length) return hide();
            tooltip.textContent = text;
            if (!tooltip.isConnected) document.body.append(tooltip);
            tooltip.hidden = false;
            const ids = (target.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean);
            target.setAttribute('aria-describedby', [...new Set([...ids, tooltip.id])].join(' '));
            const rect = target.getBoundingClientRect();
            const left = rect.left + (rect.width - tooltip.offsetWidth) / 2;
            const below = rect.bottom + 6;
            const top = below + tooltip.offsetHeight <= innerHeight - 8 ? below : rect.top - tooltip.offsetHeight - 6;
            tooltip.style.left = Math.max(8, Math.min(left, innerWidth - tooltip.offsetWidth - 8)) + 'px';
            tooltip.style.top = Math.max(8, Math.min(top, innerHeight - tooltip.offsetHeight - 8)) + 'px';
            observer.observe(document.body, { childList: true, subtree: true });
        };
        if (immediate) reveal();
        else timer = setTimeout(reveal, 250);
    }

    const targetFor = event => event.target.closest?.('[title], [data-tooltip]');
    document.addEventListener('pointerover', event => {
        const target = targetFor(event);
        if (target && !target.contains(event.relatedTarget)) show(target);
    });
    document.addEventListener('pointerout', event => {
        if (!anchor) return;
        const next = event.relatedTarget;
        if (next && (anchor.contains(next) || tooltip.contains(next))) return;
        if (anchor.contains(event.target) || tooltip.contains(event.target)) hide();
    });
    document.addEventListener('focusin', event => {
        const target = targetFor(event);
        if (target?.matches(':focus-visible')) show(target, true);
    });
    document.addEventListener('focusout', event => {
        if (anchor?.contains(event.target)) hide();
    });
    document.addEventListener('pointerdown', hide);
    document.addEventListener('scroll', hide, true);
    window.addEventListener('resize', hide);
    window.addEventListener('rcpa:theme-change', () => {
        if (anchor) show(anchor, true);
    });
    document.addEventListener('keydown', event => {
        if (event.key === 'Escape' && anchor && !tooltip.hidden) {
            const inDialog = !!anchor.closest('[role="dialog"]');
            hide();
            if (!inDialog) event.stopPropagation();
        }
    }, true);
})();
