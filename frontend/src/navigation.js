import {
  BarChart3,
  FileText,
  KeyRound,
  Layers,
  Settings,
} from 'lucide-react';

export const navItems = [
  { id: 'dashboard', path: '/dashboard', label: '仪表盘', icon: BarChart3 },
  { id: 'keys', path: '/keys', label: '密钥管理', icon: KeyRound },
  { id: 'providers', path: '/providers', label: '供应商', icon: Layers },
  { id: 'logs', path: '/logs', label: '调用日志', icon: FileText },
  { id: 'config', path: '/config', label: '配置', icon: Settings },
];
