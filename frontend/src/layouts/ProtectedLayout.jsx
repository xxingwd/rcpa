import { useState } from 'react';
import { Outlet } from 'react-router-dom';
import { Toaster } from 'sonner';
import { Menu, X } from 'lucide-react';
import { getAdminToken, setAdminToken } from '../utils/api';
import LoginOverlay from '../components/LoginOverlay';
import AdminSidebar from './AdminSidebar';
import { Button } from '../components/ui/button';
import { useToastApi } from '../hooks/use-toast-api';

export default function ProtectedLayout() {
  const [isLoggedIn, setIsLoggedIn] = useState(!!getAdminToken());
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const toastApi = useToastApi();

  const handleLogout = () => {
    setAdminToken('');
    setMobileNavOpen(false);
    setIsLoggedIn(false);
  };

  if (!isLoggedIn) {
    return (
      <>
        <LoginOverlay onLoginSuccess={() => setIsLoggedIn(true)} />
        <Toaster richColors position="bottom-right" />
      </>
    );
  }

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-foreground antialiased">
      <header className="flex h-14 shrink-0 items-center justify-between border-b bg-sidebar px-3 lg:hidden">
        <div className="flex items-center gap-2.5 text-sm font-semibold">
          <div className="flex h-8 w-8 items-center justify-center rounded-md border border-sidebar-border bg-card">R</div>
          <span>RCPA</span>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-9 w-9"
          onClick={() => setMobileNavOpen((open) => !open)}
          title={mobileNavOpen ? '关闭菜单' : '打开菜单'}
          aria-label={mobileNavOpen ? '关闭菜单' : '打开菜单'}
        >
          {mobileNavOpen ? <X size={18} /> : <Menu size={18} />}
        </Button>
      </header>

      {mobileNavOpen && (
        <div className="fixed inset-0 z-50 flex lg:hidden">
          <button
            type="button"
            className="flex-1 bg-foreground/20"
            onClick={() => setMobileNavOpen(false)}
            aria-label="关闭菜单"
          />
          <AdminSidebar
            mobile
            onLogout={handleLogout}
            onNavigate={() => setMobileNavOpen(false)}
          />
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <AdminSidebar onLogout={handleLogout} />
        <main className="min-w-0 flex-1 overflow-auto p-3 lg:overflow-hidden lg:p-4">
          <Outlet context={toastApi} />
        </main>
      </div>
      <Toaster richColors position="bottom-right" />
    </div>
  );
}
