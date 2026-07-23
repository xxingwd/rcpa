import { useState } from 'react';
import { setAdminToken } from '../utils/api';
import { toggleTheme, getTheme } from '../utils/theme';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Card, CardHeader, CardContent } from './ui/card';
import { Shield, Sun, Moon } from 'lucide-react';

export default function LoginOverlay({ onLoginSuccess }) {
  const [tokenInput, setTokenInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [errorMsg, setErrorMsg] = useState('');
  const [theme, setTheme] = useState(getTheme);

  const handleLogin = async (e) => {
    e.preventDefault();
    const token = tokenInput.trim();
    if (!token) {
      setErrorMsg('请输入控制台 Token');
      return;
    }

    setLoading(true);
      setErrorMsg('');
    try {
      const res = await fetch('/v1/admin/keys', {
        headers: { 'x-admin-token': token }
      });
      if (res.ok) {
        setAdminToken(token);
        onLoginSuccess();
      } else {
        setErrorMsg('认证失败，请检查控制台 Token');
      }
    } catch {
      setErrorMsg('无法连接至网关服务');
    } finally {
      setLoading(false);
    }
  };

  const handleThemeToggle = () => {
    const next = toggleTheme();
    setTheme(next);
  };

  return (
    <div className="fixed inset-0 bg-background z-50 flex justify-center items-center p-4">
      <Button
        variant="outline"
        size="sm"
        onClick={handleThemeToggle}
        className="absolute top-6 right-6 gap-2 text-muted-foreground"
      >
        {theme === 'dark' ? <Sun size={13} /> : <Moon size={13} />}
        <span>{theme === 'dark' ? '亮色' : '暗色'}</span>
      </Button>

      <Card className="relative w-full max-w-[400px] animate-in fade-in duration-300">
        <CardHeader className="text-center pb-2">
          <div className="w-11 h-11 mx-auto mb-4 rounded-lg border bg-muted flex justify-center items-center">
            <Shield size={21} className="text-primary" />
          </div>
          <h2 className="text-xl font-semibold">RCPA 管理登录</h2>
          <p className="text-sm text-muted-foreground">输入控制台 Token 继续</p>
        </CardHeader>

        <CardContent>
          <form onSubmit={handleLogin} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="token" className="text-xs uppercase tracking-wider text-muted-foreground">
                Token
              </Label>
              <Input
                id="token"
                type="password"
                value={tokenInput}
                onChange={(e) => setTokenInput(e.target.value)}
                placeholder="请输入控制台 Token..."
                disabled={loading}
                autoFocus
              />
            </div>

            {errorMsg && (
              <div className="text-destructive text-xs font-medium bg-destructive/10 border border-destructive/20 rounded-md px-3 py-2">
                {errorMsg}
              </div>
            )}

            <Button
              type="submit"
              disabled={loading}
              className="w-full h-10"
            >
              {loading ? '验证中...' : '登录'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
