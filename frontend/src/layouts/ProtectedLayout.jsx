import { useState } from 'react';
import { Outlet } from 'react-router-dom';
import { Toaster } from 'sonner';
import { getAdminToken, setAdminToken } from '../utils/api';
import LoginOverlay from '../components/LoginOverlay';
import AdminSidebar from './AdminSidebar';
import { useToastApi } from '../hooks/use-toast-api';

export default function ProtectedLayout() {
  const [isLoggedIn, setIsLoggedIn] = useState(!!getAdminToken());
  const toastApi = useToastApi();

  const handleLogout = () => {
    setAdminToken('');
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
    <div className="flex h-screen overflow-hidden bg-background text-foreground antialiased">
      <AdminSidebar onLogout={handleLogout} />
      <main className="min-w-0 flex-1 overflow-hidden p-6 lg:p-8">
        <Outlet context={toastApi} />
      </main>
      <Toaster richColors position="bottom-right" />
    </div>
  );
}
