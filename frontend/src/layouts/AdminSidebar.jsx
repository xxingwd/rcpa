import { useState } from 'react';
import { NavLink } from 'react-router-dom';
import { ChevronLeft, ChevronRight, LogOut, Moon, Sun } from 'lucide-react';
import { getTheme, toggleTheme } from '../utils/theme';
import { Button } from '../components/ui/button';
import { navItems } from '../navigation';

export default function AdminSidebar({ onLogout, mobile = false, onNavigate }) {
  const [theme, setTheme] = useState(getTheme);
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem('rcpa_sidebar_collapsed') === 'true'
  );

  const handleThemeToggle = () => {
    const next = toggleTheme();
    setTheme(next);
  };

  const handleCollapseToggle = () => {
    setCollapsed((current) => {
      const next = !current;
      localStorage.setItem('rcpa_sidebar_collapsed', String(next));
      return next;
    });
  };

  const isCollapsed = mobile ? false : collapsed;

  return (
    <aside
      className={`${mobile ? 'w-72' : isCollapsed ? 'w-[4.25rem] hidden lg:flex' : 'w-52 hidden lg:flex'} bg-sidebar border-r border-sidebar-border flex flex-col p-3 shrink-0 h-full lg:h-screen lg:sticky lg:top-0 transition-[width] duration-200`}
    >
      <div
        className={`flex items-center ${isCollapsed ? 'justify-center' : 'justify-between'} gap-2.5 mb-6 px-1`}
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <div className="w-8 h-8 rounded-md border border-sidebar-border bg-card flex justify-center items-center font-semibold text-sidebar-foreground text-sm">
            R
          </div>
          {!isCollapsed && (
            <div className="text-sidebar-foreground text-base font-semibold">
              RCPA
            </div>
          )}
        </div>
        {!isCollapsed && !mobile && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            title="折叠菜单"
            onClick={handleCollapseToggle}
            className="h-8 w-8 text-muted-foreground hover:text-foreground"
          >
            <ChevronLeft size={16} />
          </Button>
        )}
      </div>

      {isCollapsed && !mobile && (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          title="展开菜单"
          onClick={handleCollapseToggle}
          className="mb-3 h-9 w-full text-muted-foreground hover:text-foreground"
        >
          <ChevronRight size={17} />
        </Button>
      )}

      <nav className="flex flex-col gap-1">
        {navItems.map((item) => {
          const IconComponent = item.icon;

          return (
            <NavLink
              key={item.id}
              to={item.path}
              title={isCollapsed ? item.label : undefined}
              onClick={onNavigate}
              className={({ isActive }) =>
                `flex items-center ${isCollapsed ? 'justify-center px-0' : 'gap-2.5 px-3'} font-medium h-9 rounded-md text-sm transition-colors ${
                  isActive
                    ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                    : 'text-muted-foreground hover:text-sidebar-foreground hover:bg-accent'
                }`
              }
            >
              {({ isActive }) => (
                <>
                  <IconComponent size={17} strokeWidth={isActive ? 2.2 : 1.8} />
                  {!isCollapsed && <span>{item.label}</span>}
                </>
              )}
            </NavLink>
          );
        })}
      </nav>

      <div className="mt-auto flex flex-col gap-2 border-t border-sidebar-border pt-3">
        <Button
          variant="outline"
          size="sm"
          onClick={handleThemeToggle}
          title={isCollapsed ? (theme === 'dark' ? '亮色主题' : '暗色主题') : undefined}
          className={`${isCollapsed ? 'px-0' : 'gap-2'} w-full justify-center text-muted-foreground hover:text-foreground`}
        >
          {theme === 'dark' ? <Sun size={13} /> : <Moon size={13} />}
          {!isCollapsed && <span>{theme === 'dark' ? '亮色主题' : '暗色主题'}</span>}
        </Button>
        <Button
          variant="outline"
          size="sm"
          onClick={onLogout}
          title={isCollapsed ? '退出登录' : undefined}
          className={`${isCollapsed ? 'px-0' : 'gap-2'} w-full justify-center text-destructive hover:text-destructive border-destructive/20 hover:border-destructive/40`}
        >
          <LogOut size={13} />
          {!isCollapsed && <span>退出登录</span>}
        </Button>
      </div>
    </aside>
  );
}
