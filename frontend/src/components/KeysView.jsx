import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { apiFetch } from '../utils/api';
import { keyDisplayName, modelDisplayName } from '../utils/display';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from './ui/table';
import { Badge } from './ui/badge';
import { InlineCode } from './ui/code';
import { Switch } from './ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from './ui/dialog';
import { Copy, Edit2, KeyRound, Plus, Shuffle, Trash2 } from 'lucide-react';

export default function KeysView({ showToast }) {
  const [keys, setKeys] = useState([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modelCatalog, setModelCatalog] = useState([]);
  const [editingKey, setEditingKey] = useState(null);
  const rowIdRef = useRef(0);

  const [name, setName] = useState('');
  const [labels, setLabels] = useState('');
  const [modelRows, setModelRows] = useState([]);

  const catalogOptions = useMemo(() => {
    const seen = new Set();
    return modelCatalog.filter((entry) => {
      if (!entry?.name || seen.has(entry.name)) return false;
      seen.add(entry.name);
      return true;
    });
  }, [modelCatalog]);

  function formatCatalogLabel(entry) {
    return modelDisplayName(entry.name, 32);
  }

  function parseAliasList(value) {
    return String(value || '')
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  function hydrateModelRows(key) {
    const rows = [];
    const allowedRules = Array.isArray(key.allowed_models)
      ? key.allowed_models.filter((model) => model?.name)
      : [];
    const aliasMap = key.model_aliases || {};
    const hydratedAliases = new Set();

    allowedRules.forEach((rule) => {
      const target = aliasMap[rule.name] || rule.name;
      if (aliasMap[rule.name]) {
        hydratedAliases.add(rule.name);
      }
      rows.push({
        id: nextRowId('model'),
        modelName: target,
        aliases: aliasMap[rule.name] ? rule.name : '',
        status: rule.status || 'enabled',
      });
    });

    Object.entries(aliasMap).forEach(([alias, target]) => {
      if (!target || hydratedAliases.has(alias)) return;
      rows.push({
        id: nextRowId('model'),
        modelName: target,
        aliases: alias,
        status: 'enabled',
      });
    });

    return rows;
  }

  function nextRowId(prefix) {
    rowIdRef.current += 1;
    return `${prefix}-${rowIdRef.current}`;
  }

  const copyToClipboard = (text) => {
    navigator.clipboard.writeText(text).then(
      () => showToast('密钥已复制到剪贴板', 'success'),
      () => showToast('复制失败，请手动复制', 'error')
    );
  };

  const fetchKeys = useCallback(async () => {
    try {
      const res = await apiFetch('/v1/admin/keys');
      if (res.ok) {
        const data = await res.json();
        setKeys(Array.isArray(data) ? data : []);
      }
    } catch {
      showToast('获取 API 密钥失败', 'error');
    }
  }, [showToast]);

  const fetchModelCatalog = useCallback(async () => {
    try {
      const res = await apiFetch('/v1/admin/model-catalog');
      if (!res.ok) {
        setModelCatalog([]);
        return [];
      }
      const catalog = await res.json();
      const list = Array.isArray(catalog) ? catalog : [];
      setModelCatalog(list);
      return list;
    } catch {
      showToast('获取模型目录失败', 'error');
      return [];
    }
  }, [showToast]);

  useEffect(() => {
    fetchKeys();
  }, [fetchKeys]);

  const openCreateModal = async () => {
    await fetchModelCatalog();
    setEditingKey(null);
    setName('');
    setLabels('');
    setModelRows([]);
    setIsModalOpen(true);
  };

  const openEditModal = async (key) => {
    await fetchModelCatalog();
    setEditingKey(key);
    setName(key.name || '');
    setLabels(key.labels || '');
    setModelRows(hydrateModelRows(key));
    setIsModalOpen(true);
  };

  const buildKeyModelPayload = () => {
    const seen = new Set();
    const allowedModels = [];
    const modelAliases = {};

    modelRows.forEach((row) => {
      const modelName = String(row.modelName || '').trim();
      if (!modelName) return;
      const status = row.status || 'enabled';

      const aliases = parseAliasList(row.aliases);
      if (aliases.length > 0) {
        aliases.forEach((alias) => {
          modelAliases[alias] = modelName;
          if (!seen.has(alias)) {
            seen.add(alias);
            allowedModels.push({ name: alias, status });
          }
        });
        return;
      }

      if (!seen.has(modelName)) {
        seen.add(modelName);
        allowedModels.push({ name: modelName, status });
      }
    });

    return { allowedModels, modelAliases };
  };

  const addModelRow = () => {
    const entry = catalogOptions[0];
    setModelRows([
      ...modelRows,
      {
        id: nextRowId('model'),
        modelName: entry?.name || '',
        aliases: '',
        status: 'enabled',
      },
    ]);
  };

  const updateModelRow = (id, patch) => {
    setModelRows(modelRows.map((row) => (row.id === id ? { ...row, ...patch } : row)));
  };

  const removeModelRow = (id) => {
    setModelRows(modelRows.filter((row) => row.id !== id));
  };

  const handleSaveKey = async (e) => {
    e.preventDefault();
    const { allowedModels, modelAliases } = buildKeyModelPayload();
    const payload = {
      name: name.trim() || null,
      allowed_models: editingKey
        ? allowedModels
        : (allowedModels.length > 0 ? allowedModels : null),
      labels: labels.trim() || null,
      model_aliases: modelAliases,
    };

    try {
      const res = await apiFetch(editingKey ? `/v1/admin/keys/${encodeURIComponent(editingKey.id)}` : '/v1/admin/keys', {
        method: editingKey ? 'PUT' : 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (res.ok) {
        showToast(editingKey ? 'API 密钥配置已更新' : 'API 密钥生成成功', 'success');
        setIsModalOpen(false);
        fetchKeys();
      } else {
        const data = await res.json().catch(() => null);
        showToast(data?.error?.message || (editingKey ? '更新 API 密钥失败' : '生成密钥失败'), 'error');
      }
    } catch {
      showToast('API 接口连接出错。', 'error');
    }
  };

  const toggleKeyStatus = async (id, currentStatus) => {
    const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
    try {
      const res = await apiFetch(`/v1/admin/keys/${encodeURIComponent(id)}/status`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status: nextStatus }),
      });
      if (res.ok) {
        showToast(`API 密钥已${nextStatus === 'enabled' ? '启用' : '禁用'}`, 'success');
        fetchKeys();
      } else {
        showToast('更新 API 密钥状态失败', 'error');
      }
    } catch {
      showToast('API 接口连接出错。', 'error');
    }
  };

  const toggleKeyModelStatus = async (keyId, modelName, currentStatus) => {
    const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
    try {
      const res = await apiFetch(`/v1/admin/keys/${encodeURIComponent(keyId)}/models/${encodeURIComponent(modelName)}/status`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status: nextStatus }),
      });
      if (res.ok) {
        showToast(`模型规则 ${modelName} 已${nextStatus === 'enabled' ? '启用' : '禁用'}`, 'success');
        fetchKeys();
      } else {
        showToast('更新模型规则失败', 'error');
      }
    } catch {
      showToast('API 接口连接出错。', 'error');
    }
  };

  return (
    <Card className="animate-in fade-in duration-500">
      <CardHeader>
        <div className="flex justify-between items-center">
          <CardTitle>API 密钥管理</CardTitle>
          <Button onClick={openCreateModal}>
            <Plus size={15} />
            <span>生成新密钥</span>
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>名称</TableHead>
              <TableHead>API 密钥</TableHead>
              <TableHead>状态</TableHead>
              <TableHead>允许模型</TableHead>
              <TableHead>模型别名</TableHead>
              <TableHead>备注</TableHead>
              <TableHead>操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {keys.length > 0 ? (
              keys.map((key) => {
                const modelRules = Array.isArray(key.allowed_models) ? key.allowed_models : [];
                const aliasEntries = Object.entries(key.model_aliases || {});
                const keyValue = key.key || '';
                const isEnabled = key.status === 'enabled';
                const displayName = keyDisplayName(key) || '—';

                return (
                  <TableRow key={key.id}>
                    <TableCell className="font-medium">{displayName}</TableCell>
                    <TableCell className="min-w-[320px]">
                      <div className="flex items-center gap-1.5">
                        <InlineCode className="select-all flex-1">
                          {keyValue}
                        </InlineCode>
                        <Button
                          variant="outline"
                          size="icon"
                          className="h-7 w-7 shrink-0 text-primary"
                          onClick={() => copyToClipboard(keyValue)}
                          title="复制密钥"
                        >
                          <Copy size={13} />
                        </Button>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant={isEnabled ? 'success' : 'destructive'}>
                        {isEnabled ? '启用' : '禁用'}
                      </Badge>
                    </TableCell>
                    <TableCell className="max-w-[200px]">
                      {modelRules.length === 0 ? (
                        <span className="text-muted-foreground text-xs italic">全部</span>
                      ) : (
                        <div className="flex flex-wrap gap-1">
                          {modelRules.map((model) => (
                            <Button
                              key={model.name}
                              variant="outline"
                              size="sm"
                              className={`h-6 px-2 font-mono text-xs ${model.status === 'enabled' ? '' : 'opacity-50 line-through'}`}
                              onClick={() => toggleKeyModelStatus(key.id, model.name, model.status)}
                              title={model.name}
                            >
                              {modelDisplayName(model.name, 24)}
                            </Button>
                          ))}
                        </div>
                      )}
                    </TableCell>
                    <TableCell className="max-w-[180px]">
                      {aliasEntries.length === 0 ? (
                        <span className="text-muted-foreground text-xs italic">—</span>
                      ) : (
                        <div className="flex flex-wrap gap-1">
                          {aliasEntries.map(([alias, target]) => (
                            <Badge key={alias} variant="secondary" className="max-w-full font-mono text-xs" title={`${alias} -> ${target}`}>
                              <span className="truncate">
                                {modelDisplayName(alias, 14)} -&gt; {modelDisplayName(target, 16)}
                              </span>
                            </Badge>
                          ))}
                        </div>
                      )}
                    </TableCell>
                    <TableCell>{key.labels || '—'}</TableCell>
                    <TableCell>
                      <div className="flex gap-1.5">
                        <Button variant="outline" size="icon" className="h-7 w-7" onClick={() => openEditModal(key)}>
                          <Edit2 size={12} />
                        </Button>
                        <Button variant="outline" size="sm" className="h-7 text-xs" onClick={() => toggleKeyStatus(key.id, key.status)}>
                          {isEnabled ? '禁用' : '启用'}
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })
            ) : (
              <TableRow>
                <TableCell colSpan={7} className="text-center text-muted-foreground py-8">未找到 API 密钥</TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </CardContent>

      <Dialog open={isModalOpen} onOpenChange={setIsModalOpen}>
        <DialogContent className="max-w-[760px]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <KeyRound size={18} className="text-primary" />
              <span>{editingKey ? '编辑 API 密钥' : '生成 API 密钥'}</span>
            </DialogTitle>
            <DialogDescription>
              配置 API 密钥的名称、允许访问的模型、私有模型别名和备注。
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={handleSaveKey} className="space-y-4">
            {editingKey && (
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">API 密钥</Label>
                <Input type="text" value={editingKey.key || ''} readOnly className="font-mono" />
              </div>
            )}

            <div className="space-y-2">
              <Label className="text-xs uppercase tracking-wider text-muted-foreground">名称</Label>
              <Input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder="生产环境密钥" />
            </div>

            <div className="border-t border-border pt-4">
              <div className="flex items-center justify-between mb-3">
                <h4 className="text-sm font-semibold flex items-center gap-1.5">
                  模型 / Key 别名
                </h4>
                <Button type="button" variant="outline" size="sm" onClick={addModelRow} disabled={catalogOptions.length === 0} className="gap-1 text-xs">
                  <Plus size={11} /> 添加模型
                </Button>
              </div>

              <div className="space-y-2 max-h-[300px] overflow-y-auto pr-1">
                {modelRows.map((row) => {
                  return (
                    <div key={row.id} className="flex gap-2 items-end bg-muted/50 border border-border p-3 rounded-lg">
                      <div className="flex-1 grid grid-cols-12 gap-2">
                        <div className="col-span-5 space-y-1">
                          <Label className="text-[10px] text-muted-foreground">有效模型名 *</Label>
                          <Select
                            value={row.modelName}
                            onValueChange={(value) => {
                              updateModelRow(row.id, {
                                modelName: value,
                              });
                            }}
                          >
                            <SelectTrigger className="font-mono">
                              <SelectValue placeholder="选择模型名" />
                            </SelectTrigger>
                            <SelectContent>
                              {catalogOptions.map((entryOption) => (
                                <SelectItem key={entryOption.name} value={entryOption.name} className="font-mono text-xs" title={entryOption.name}>
                                  {formatCatalogLabel(entryOption)}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>
                        <div className="col-span-4 space-y-1">
                          <Label className="text-[10px] text-muted-foreground flex items-center gap-0.5"><Shuffle size={9} /> Key 别名</Label>
                          <Input
                            type="text"
                            value={row.aliases}
                            onChange={(e) => updateModelRow(row.id, { aliases: e.target.value })}
                            placeholder="fast,quick"
                            className="font-mono"
                          />
                        </div>
                        <div className="col-span-3 space-y-1">
                          <Label className="text-[10px] text-muted-foreground">状态</Label>
                          <div className="flex h-9 items-center gap-2">
                            <Switch
                              checked={row.status !== 'disabled'}
                              onCheckedChange={(checked) => updateModelRow(row.id, { status: checked ? 'enabled' : 'disabled' })}
                              title={row.status !== 'disabled' ? '启用' : '禁用'}
                            />
                            <span className={`text-xs ${row.status !== 'disabled' ? 'text-emerald-600 dark:text-emerald-400' : 'text-muted-foreground'}`}>
                              {row.status !== 'disabled' ? '启用' : '禁用'}
                            </span>
                          </div>
                        </div>
                      </div>
                      <Button type="button" variant="destructive" size="icon" className="h-8 w-8 shrink-0" onClick={() => removeModelRow(row.id)}>
                        <Trash2 size={13} />
                      </Button>
                    </div>
                  );
                })}
                {modelRows.length === 0 && (
                  <div className="text-center py-5 border border-dashed border-border rounded-lg text-xs text-muted-foreground">
                    点击"添加模型"开始配置；为空时默认允许全部模型
                  </div>
                )}
                {catalogOptions.length === 0 && (
                  <span className="block text-xs text-muted-foreground">暂无可用模型</span>
                )}
              </div>
            </div>

            <div className="space-y-2">
              <Label className="text-xs uppercase tracking-wider text-muted-foreground">备注</Label>
              <Input type="text" value={labels} onChange={(e) => setLabels(e.target.value)} />
            </div>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsModalOpen(false)}>取消</Button>
              <Button type="submit">{editingKey ? '保存配置' : '生成密钥'}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
