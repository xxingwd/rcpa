import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../utils/api';
import { modelDisplayName } from '../utils/display';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from './ui/table';
import { Badge } from './ui/badge';
import { Checkbox } from './ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from './ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './ui/select';
import { Coins, Edit2, Layers, Plus, Shuffle, Trash2 } from 'lucide-react';

const emptyModel = {
  name: '',
  status: 'enabled',
  inputPrice: '',
  outputPrice: '',
  aliases: '',
};

const ADAPTER_OPTIONS = [
  { value: 'openai', protocols: ['completions', 'responses'] },
  { value: 'anthropic', protocols: ['messages'] },
];

function modelNames(provider) {
  return Array.isArray(provider.models) ? provider.models : [];
}

function adapterProtocols(adapter) {
  return ADAPTER_OPTIONS.find((option) => option.value === adapter)?.protocols || [];
}

function normalizeProtocols(adapter, values) {
  const allowed = adapterProtocols(adapter);
  const filtered = (Array.isArray(values) ? values : []).filter((value) => allowed.includes(value));
  if (filtered.length > 0) {
    return filtered;
  }
  return allowed.length > 0 ? [allowed[0]] : [];
}

export default function ProvidersView({ showToast }) {
  const [providers, setProviders] = useState([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState(null);

  const [name, setName] = useState('');
  const [adapter, setAdapter] = useState('openai');
  const [protocols, setProtocols] = useState(['completions']);
  const [url, setUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [weight, setWeight] = useState(10);
  const [maxConn, setMaxConn] = useState(100);
  const [timeout, setTimeoutSecs] = useState(300);
  const [priority, setPriority] = useState(0);
  const [group, setGroup] = useState('default');
  const [apiVersion, setApiVersion] = useState('');
  const [formModels, setFormModels] = useState([]);

  const availableProtocols = useMemo(() => adapterProtocols(adapter), [adapter]);

  const fetchProviders = useCallback(async () => {
    try {
      const providerRes = await apiFetch('/v1/admin/providers');
      if (providerRes.ok) {
        const data = await providerRes.json();
        setProviders(Array.isArray(data) ? data : []);
      }
    } catch {
      showToast('获取供应商配置失败', 'error');
    }
  }, [showToast]);

  useEffect(() => {
    fetchProviders();
  }, [fetchProviders]);

  const openCreateModal = () => {
    setEditingProvider(null);
    setName('');
    setAdapter('openai');
    setProtocols(['completions']);
    setUrl('');
    setApiKey('');
    setWeight(10);
    setMaxConn(100);
    setTimeoutSecs(300);
    setPriority(0);
    setGroup('default');
    setApiVersion('');
    setFormModels([]);
    setIsModalOpen(true);
  };

  const openEditModal = (provider) => {
    const nextAdapter = provider.adapter || 'openai';
    setEditingProvider(provider);
    setName(provider.name);
    setAdapter(nextAdapter);
    setProtocols(normalizeProtocols(nextAdapter, provider.protocols));
    setUrl(provider.base_url);
    setApiKey(provider.api_key || '');
    setWeight(provider.weight);
    setMaxConn(provider.max_connections);
    setTimeoutSecs(provider.timeout_secs);
    setPriority(provider.priority);
    setGroup(provider.group);
    setApiVersion(provider.api_version || '');
    setFormModels(modelNames(provider).map((model) => ({
      name: model.name,
      status: model.status || 'enabled',
      inputPrice: model.pricing?.input_per_1k?.toString() || '',
      outputPrice: model.pricing?.output_per_1k?.toString() || '',
      aliases: Array.isArray(model.aliases) ? model.aliases.join(', ') : '',
    })));
    setIsModalOpen(true);
  };

  const addModelRow = () => setFormModels([...formModels, { ...emptyModel }]);

  const removeModelRow = (i) => setFormModels(formModels.filter((_, idx) => idx !== i));

  const handleModelChange = (i, field, value) => {
    const next = [...formModels];
    next[i] = { ...next[i], [field]: value };
    setFormModels(next);
  };

  const handleAdapterChange = (value) => {
    setAdapter(value);
    setProtocols((current) => normalizeProtocols(value, current));
  };

  const toggleProtocol = (value, checked) => {
    setProtocols((current) => {
      const next = checked
        ? Array.from(new Set([...current, value]))
        : current.filter((item) => item !== value);
      return normalizeProtocols(adapter, next);
    });
  };

  const buildModelRules = () => formModels
    .map((model) => {
      const trimmedName = model.name.trim();
      if (!trimmedName) return null;
      const input = parseFloat(model.inputPrice);
      const output = parseFloat(model.outputPrice);
      const pricing = Number.isFinite(input) && Number.isFinite(output)
        ? { input_per_1k: input, output_per_1k: output }
        : null;
      return {
        name: trimmedName,
        status: model.status || 'enabled',
        pricing,
        aliases: model.aliases.split(',').map((value) => value.trim()).filter(Boolean),
      };
    })
    .filter(Boolean);

  const handleSaveProvider = async (e) => {
    e.preventDefault();
    const providerName = name.trim();
    const baseUrl = url.trim();
    const key = apiKey.trim();
    if (!providerName || !baseUrl || !key) {
      showToast('请完整填写供应商基本信息 (*)', 'error');
      return;
    }

    const models = buildModelRules();
    if (models.length === 0) {
      showToast('请至少添加一个有效的模型名称', 'error');
      return;
    }

    const selectedProtocols = normalizeProtocols(adapter, protocols);
    if (selectedProtocols.length === 0) {
      showToast('请至少选择一个协议', 'error');
      return;
    }

    const payload = {
      name: providerName,
      adapter,
      protocols: selectedProtocols,
      base_url: baseUrl,
      api_key: key,
      models,
      weight: parseInt(weight, 10) || 10,
      max_connections: parseInt(maxConn, 10) || 100,
      timeout_secs: parseInt(timeout, 10) || 300,
      priority: parseInt(priority, 10) || 0,
      group: group.trim() || 'default',
      api_version: apiVersion.trim() || null,
      status: editingProvider?.status || 'enabled',
    };

    try {
      const res = await apiFetch('/v1/admin/providers', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      if (!res.ok) {
        const errData = await res.json().catch(() => ({}));
        throw new Error(errData.error?.message || '保存供应商失败');
      }
      showToast(editingProvider ? '供应商更新成功' : '供应商注册成功', 'success');
      setIsModalOpen(false);
      fetchProviders();
    } catch (err) {
      showToast(err.message || '保存供应商失败，请检查配置。', 'error');
    }
  };

  const handleDeleteProvider = async (providerName) => {
    if (!confirm(`确定删除供应商 ${providerName}？`)) return;
    try {
      const res = await apiFetch(`/v1/admin/providers/${encodeURIComponent(providerName)}`, { method: 'DELETE' });
      if (res.ok) {
        showToast(`供应商 ${providerName} 已删除`, 'success');
        fetchProviders();
      } else {
        showToast('删除供应商失败', 'error');
      }
    } catch {
      showToast('API 接口连接出错。', 'error');
    }
  };

  const toggleProviderStatus = async (providerName, currentStatus) => {
    const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
    try {
      const res = await apiFetch(`/v1/admin/providers/${encodeURIComponent(providerName)}/status`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status: nextStatus }),
      });
      if (res.ok) {
        showToast(`供应商 ${providerName} 已${nextStatus === 'enabled' ? '启用' : '禁用'}`, 'success');
        fetchProviders();
      } else {
        showToast('更新供应商状态失败', 'error');
      }
    } catch {
      showToast('API 接口连接出错。', 'error');
    }
  };

  const toggleProviderModelStatus = async (providerName, modelName, currentStatus) => {
    const nextStatus = currentStatus === 'enabled' ? 'disabled' : 'enabled';
    try {
      const res = await apiFetch(`/v1/admin/providers/${encodeURIComponent(providerName)}/models/${encodeURIComponent(modelName)}/status`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status: nextStatus }),
      });
      if (res.ok) {
        showToast(`模型 ${modelName} 已${nextStatus === 'enabled' ? '启用' : '禁用'}`, 'success');
        fetchProviders();
      } else {
        showToast('更新模型状态失败', 'error');
      }
    } catch {
      showToast('API 接口连接出错。', 'error');
    }
  };

  return (
    <Card className="animate-in fade-in duration-500">
      <CardHeader>
        <div className="flex justify-between items-center">
          <CardTitle>供应商与模型配置</CardTitle>
          <Button onClick={openCreateModal}>
            <Plus size={15} />
            <span>注册供应商</span>
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>供应商</TableHead>
              <TableHead>适配器 / 协议</TableHead>
              <TableHead>状态</TableHead>
              <TableHead>权重</TableHead>
              <TableHead>并发 / 超时</TableHead>
              <TableHead>模型</TableHead>
              <TableHead>操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {providers.length > 0 ? (
              providers.map((provider) => {
                const isEnabled = provider.status === 'enabled';
                return (
                  <TableRow key={provider.name}>
                    <TableCell className="font-semibold">{provider.name}</TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1.5">
                        <Badge variant="outline" className="font-mono">{provider.adapter}</Badge>
                        {(provider.protocols || []).map((value) => (
                          <Badge key={value} variant="secondary" className="font-mono">
                            {value}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant={isEnabled ? 'success' : 'destructive'}>
                        {isEnabled ? '启用' : '禁用'}
                      </Badge>
                    </TableCell>
                    <TableCell className="font-mono">{provider.weight}</TableCell>
                    <TableCell>{provider.max_connections} <span className="text-muted-foreground text-xs">/ {provider.timeout_secs}s</span></TableCell>
                    <TableCell className="max-w-[320px]">
                      <div className="flex flex-wrap gap-1.5">
                        {modelNames(provider).map((model) => {
                          const modelEnabled = model.status === 'enabled';
                          return (
                            <button
                              key={model.name}
                              type="button"
                              onClick={() => toggleProviderModelStatus(provider.name, model.name, model.status)}
                              title={model.name}
                              className={`
                                inline-flex items-center gap-1.5 h-6 px-2 rounded-full font-mono text-xs border transition-colors
                                ${modelEnabled
                                  ? 'bg-emerald-500/10 border-emerald-500/40 text-emerald-600 hover:bg-emerald-500/20'
                                  : 'bg-muted border-border text-muted-foreground hover:bg-muted/80'}
                              `}
                            >
                              <span className={`w-1.5 h-1.5 rounded-full ${modelEnabled ? 'bg-emerald-500' : 'bg-muted-foreground/50'}`} />
                              {modelDisplayName(model.name, 24)}
                            </button>
                          );
                        })}
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex gap-1.5">
                        <Button variant="outline" size="icon" className="h-7 w-7" onClick={() => openEditModal(provider)}>
                          <Edit2 size={12} />
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => toggleProviderStatus(provider.name, provider.status)}
                          className="h-7 text-xs"
                        >
                          {isEnabled ? '禁用' : '启用'}
                        </Button>
                        <Button variant="destructive" size="sm" className="h-7 text-xs" onClick={() => handleDeleteProvider(provider.name)}>删除</Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })
            ) : (
              <TableRow>
                <TableCell colSpan={7} className="text-center text-muted-foreground py-8">暂无配置的供应商</TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </CardContent>

      <Dialog open={isModalOpen} onOpenChange={setIsModalOpen}>
        <DialogContent className="max-w-[920px] max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Layers size={18} className="text-primary" />
              <span>{editingProvider ? '编辑供应商' : '注册新供应商'}</span>
            </DialogTitle>
            <DialogDescription>
              配置供应商的连接信息、协议能力、模型列表、模型别名和计费规则。
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={handleSaveProvider} className="space-y-5">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">供应商名称 *</Label>
                <Input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder="primary-provider" required disabled={!!editingProvider} />
              </div>
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">适配器 *</Label>
                <Select value={adapter} onValueChange={handleAdapterChange}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {ADAPTER_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>{option.value}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="space-y-2">
              <Label className="text-xs uppercase tracking-wider text-muted-foreground">协议能力 *</Label>
              <div className="flex flex-wrap gap-3 rounded-lg border border-border bg-muted/40 p-3">
                {availableProtocols.map((value) => (
                  <label key={value} className="flex items-center gap-2 text-sm">
                    <Checkbox
                      checked={protocols.includes(value)}
                      onCheckedChange={(checked) => toggleProtocol(value, checked === true)}
                    />
                    <span className="font-mono">{value}</span>
                  </label>
                ))}
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">Base URL *</Label>
                <Input type="text" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://api.example.com" required />
              </div>
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">API Key *</Label>
                <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-..." required />
              </div>
            </div>

            <div className="grid grid-cols-3 gap-3 bg-muted/50 p-3 rounded-lg border border-border">
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">权重</Label>
                <Input type="number" value={weight} onChange={(e) => setWeight(parseInt(e.target.value, 10))} />
              </div>
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">最大并发</Label>
                <Input type="number" value={maxConn} onChange={(e) => setMaxConn(parseInt(e.target.value, 10))} />
              </div>
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">超时 (秒)</Label>
                <Input type="number" value={timeout} onChange={(e) => setTimeoutSecs(parseInt(e.target.value, 10))} />
              </div>
            </div>

            <div className="grid grid-cols-3 gap-3 bg-muted/50 p-3 rounded-lg border border-border">
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">优先级</Label>
                <Input type="number" value={priority} onChange={(e) => setPriority(parseInt(e.target.value, 10))} />
              </div>
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">分组</Label>
                <Input type="text" value={group} onChange={(e) => setGroup(e.target.value)} />
              </div>
              <div className="space-y-2">
                <Label className="text-xs text-muted-foreground">API Version</Label>
                <Input type="text" value={apiVersion} onChange={(e) => setApiVersion(e.target.value)} placeholder="2024-02-01" />
              </div>
            </div>

            <div className="border-t border-border pt-4">
              <div className="flex justify-between items-center mb-3">
                <h4 className="text-sm font-semibold flex items-center gap-1.5">
                  模型 / 费率 / 别名
                </h4>
                <Button type="button" variant="outline" size="sm" onClick={addModelRow} className="gap-1 text-xs">
                  <Plus size={11} /> 添加模型
                </Button>
              </div>

              <div className="space-y-2 max-h-[300px] overflow-y-auto pr-1">
                {formModels.map((item, idx) => (
                  <div key={idx} className="flex gap-2 items-end bg-muted/50 border border-border p-3 rounded-lg">
                    <div className="flex-1 grid grid-cols-12 gap-2">
                      <div className="col-span-4 space-y-1">
                        <Label className="text-[10px] text-muted-foreground">模型名称 *</Label>
                        <Input type="text" value={item.name} onChange={(e) => handleModelChange(idx, 'name', e.target.value)} placeholder="gpt-4o-mini" required />
                      </div>
                      <div className="col-span-3 space-y-1">
                        <Label className="text-[10px] text-muted-foreground flex items-center gap-0.5"><Shuffle size={9} /> 别名</Label>
                        <Input type="text" value={item.aliases} onChange={(e) => handleModelChange(idx, 'aliases', e.target.value)} placeholder="gpt4,my-gpt" className="font-mono" />
                      </div>
                      <div className="col-span-2 space-y-1">
                        <Label className="text-[10px] text-muted-foreground flex items-center gap-0.5"><Coins size={9} /> 输入/1K</Label>
                        <Input type="number" step="0.000001" value={item.inputPrice} onChange={(e) => handleModelChange(idx, 'inputPrice', e.target.value)} placeholder="0.005" />
                      </div>
                      <div className="col-span-2 space-y-1">
                        <Label className="text-[10px] text-muted-foreground flex items-center gap-0.5"><Coins size={9} /> 输出/1K</Label>
                        <Input type="number" step="0.000001" value={item.outputPrice} onChange={(e) => handleModelChange(idx, 'outputPrice', e.target.value)} placeholder="0.015" />
                      </div>
                      <div className="col-span-1 space-y-1">
                        <Label className="text-[10px] text-muted-foreground">状态</Label>
                        <Select value={item.status} onValueChange={(value) => handleModelChange(idx, 'status', value)}>
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="enabled">启用</SelectItem>
                            <SelectItem value="disabled">禁用</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                    </div>
                    <Button type="button" variant="destructive" size="icon" className="h-8 w-8 shrink-0" onClick={() => removeModelRow(idx)}>
                      <Trash2 size={13} />
                    </Button>
                  </div>
                ))}
                {formModels.length === 0 && (
                  <div className="text-center py-5 border border-dashed border-border rounded-lg text-xs text-muted-foreground">
                    点击"添加模型"开始配置
                  </div>
                )}
              </div>
            </div>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setIsModalOpen(false)}>取消</Button>
              <Button type="submit">保存配置</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
