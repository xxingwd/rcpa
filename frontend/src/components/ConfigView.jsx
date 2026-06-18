import { useCallback, useEffect, useMemo, useState } from 'react';
import { RefreshCw, Save, Settings } from 'lucide-react';
import { apiFetch } from '../utils/api';
import { Button } from './ui/button';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';

export default function ConfigView({ showToast }) {
  const [path, setPath] = useState('');
  const [content, setContent] = useState('');
  const [savedContent, setSavedContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  const isDirty = useMemo(() => content !== savedContent, [content, savedContent]);

  const fetchConfig = useCallback(async () => {
    setLoading(true);
    try {
      const res = await apiFetch('/v1/admin/config-file');
      if (!res.ok) {
        const data = await res.json().catch(() => null);
        throw new Error(data?.error?.message || '读取配置失败');
      }
      const data = await res.json();
      setPath(data.path || '');
      setContent(data.content || '');
      setSavedContent(data.content || '');
    } catch (err) {
      showToast(err.message || '读取配置失败', 'error');
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  const saveConfig = async () => {
    setSaving(true);
    try {
      const res = await apiFetch('/v1/admin/config-file', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content }),
      });
      const data = await res.json().catch(() => null);
      if (!res.ok) {
        throw new Error(data?.error?.message || '保存配置失败');
      }
      setSavedContent(content);
      if (data?.path) setPath(data.path);
      showToast('配置已保存', 'success');
    } catch (err) {
      showToast(err.message || '保存配置失败', 'error');
    } finally {
      setSaving(false);
    }
  };

  const handleEditorKeyDown = (event) => {
    if (event.key !== 'Tab') return;
    event.preventDefault();
    const target = event.currentTarget;
    const start = target.selectionStart;
    const end = target.selectionEnd;
    const next = `${content.slice(0, start)}  ${content.slice(end)}`;
    setContent(next);
    requestAnimationFrame(() => {
      target.selectionStart = start + 2;
      target.selectionEnd = start + 2;
    });
  };

  return (
    <Card className="flex h-full min-h-0 flex-col overflow-hidden animate-in fade-in duration-500">
      <CardHeader className="shrink-0 border-b py-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <CardTitle className="flex items-center gap-2">
            <Settings size={18} className="text-primary" />
            配置
          </CardTitle>
          <div className="flex flex-wrap items-center gap-2">
            {path && (
              <Badge variant="outline" className="max-w-[min(32rem,calc(100vw-18rem))] truncate font-mono">
                {path}
              </Badge>
            )}
            {isDirty && <Badge variant="warning">已修改</Badge>}
            <Button type="button" variant="outline" size="sm" onClick={fetchConfig} disabled={loading || saving}>
              <RefreshCw size={13} />
              重载
            </Button>
            <Button type="button" size="sm" onClick={saveConfig} disabled={loading || saving || !isDirty}>
              <Save size={13} />
              保存
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="min-h-0 flex-1 p-0">
        <textarea
          value={content}
          onChange={(event) => setContent(event.target.value)}
          onKeyDown={handleEditorKeyDown}
          spellCheck={false}
          className="h-full w-full resize-none border-0 bg-background p-4 font-mono text-xs leading-5 text-foreground outline-none selection:bg-primary/20"
        />
      </CardContent>
    </Card>
  );
}
