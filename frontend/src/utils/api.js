let adminToken = localStorage.getItem('rcpa_admin_token') || '';

export function getAdminToken() {
    return adminToken;
}

export function setAdminToken(token) {
    adminToken = token;
    if (token) {
        localStorage.setItem('rcpa_admin_token', token);
    } else {
        localStorage.removeItem('rcpa_admin_token');
    }
}

export async function apiFetch(url, options = {}) {
    options.headers = options.headers || {};
    if (adminToken) {
        options.headers['x-admin-token'] = adminToken;
    }
    
    const res = await fetch(url, options);
    if (res.status === 401) {
        setAdminToken('');
        // Reload to force login screen
        window.location.reload();
        throw new Error('Unauthorized');
    }
    return res;
}
