import { Navigate, Route, Routes } from 'react-router-dom';
import ProtectedLayout from '../layouts/ProtectedLayout';
import DashboardPage from '../pages/DashboardPage';
import KeysPage from '../pages/KeysPage';
import ProvidersPage from '../pages/ProvidersPage';
import LogsPage from '../pages/LogsPage';
import ConfigPage from '../pages/ConfigPage';

export default function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<ProtectedLayout />}>
        <Route index element={<Navigate to="/dashboard" replace />} />
        <Route path="dashboard" element={<DashboardPage />} />
        <Route path="keys" element={<KeysPage />} />
        <Route path="providers" element={<ProvidersPage />} />
        <Route path="logs" element={<LogsPage />} />
        <Route path="config" element={<ConfigPage />} />
      </Route>
      <Route path="*" element={<Navigate to="/dashboard" replace />} />
    </Routes>
  );
}
