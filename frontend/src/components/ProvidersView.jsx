import { useCallback, useEffect, useState } from 'react';
import { apiFetch } from '../utils/api';
import { modelDisplayName } from '../utils/display';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from './ui/table';
import { Badge } from './ui/badge';
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

const PROTOCOL_OPTIONS = ['completions', 'responses', 'messages'];

const emptyModel = {
  name: '',
  status: 'enabled',
  inputPrice: '',
  outputPrice: '',
  aliases: '',
};

const emptyEndpoint = {
  protocol: '',
  base_url: '',
};

const emptyHeader = {
  name: '',
  value: '',
};

function modelNames(provider) {
  return Array.isArray(provider.models) ? provider.models : [];
}

export default function ProvidersView({ showToast }) {
  const [providers, setProviders] = useState([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState(null);

  const [name, setName] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [providerStatus, setProviderStatus] = useState('enabled');
  const [endpoints, setEndpoints] = useState([]);
  const [priority, setPriority] = useState(0);
  const [formModels, setFormModels] = useState([]);
  const [customHeaders, setCustomHeaders] = useState([]);

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
    setApiKey('');
    setEndpoints([]);
    setPriority(0);
    setProviderStatus('enabled');
    setFormModels([]);
    setCustomHeaders([]);
    setIsModalOpen(true);
  };

  const openEditModal = (provider) => {
    setEditingProvider(provider);
    setName(provider.name || '');
    setApiKey(provider.api_key || '');
    setEndpoints(Array.isArray(provider.endpoints) ? provider.endpoints : []);
    setPriority(provider.priority ?? 0);
    setProviderStatus(provider.status || 'enabled');
    setCustomHeaders(Object.entries(provider.headers || {}).map(([headerName, value]) => ({
      name: headerName,
      value,
    })));
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
  const addEndpointRow = () => setEndpoints([...endpoints, { ...emptyEndpoint }]);
  const addHeaderRow = () => setCustomHeaders([...customHeaders, { ...emptyHeader }]);

  const removeModelRow = (index) => {
    setFormModels(formModels.filter((_, idx) => idx !== index));
  };

  const removeEndpointRow = (index) => {
    setEndpoints(endpoints.filter((_, idx) => idx !== index));
  };

  const removeHeaderRow = (index) => {
    setCustomHeaders(customHeaders.filter((_, idx) => idx !== index));
  };

  const handleModelChange = (index, field, value) => {
    const next = [...formModels];
    next[index] = { ...next[index], [field]: value };
    setFormModels(next);
  };

  const handleEndpointChange = (index, field, value) => {
    const next = [...endpoints];
    next[index] = { ...next[index], [field]: value };
    setEndpoints(next);
  };

  const handleHeaderChange = (index, field, value) => {
    const next = [...customHeaders];
    next[index] = { ...next[index], [field]: value };
    setCustomHeaders(next);
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

  const handleSaveProvider = async (event) => {
    event.preventDefault();

    const providerName = name.trim();
    const key = apiKey.trim();
    if (!providerName || !key) {
      showToast('请完整填写供应商名称和 API Key', 'error');
      return;
    }

    const normalizedEndpoints = endpoints
      .map((endpoint) => ({
        protocol: endpoint.protocol,
        base_url: endpoint.base_url.trim(),
      }))
      .filter((endpoint) => endpoint.protocol || endpoint.base_url);

    if (normalizedEndpoints.length === 0) {
      showToast('请至少添加一个 endpoint', 'error');
      return;
    }

    if (normalizedEndpoints.some((endpoint) => !endpoint.protocol || !endpoint.base_url)) {
      showToast('请为每个 endpoint 完整填写协议和 Base URL', 'error');
      return;
    }

    const protocolSet = new Set(normalizedEndpoints.map((endpoint) => endpoint.protocol));
    if (protocolSet.size !== normalizedEndpoints.length) {
      showToast('同一供应商的 endpoint 协议不能重复', 'error');
      return;
    }

    const models = buildModelRules();
    if (models.length === 0) {
      showToast('请至少添加一个有效的模型名称', 'error');
      return;
    }

    const normalizedHeaders = customHeaders
      .map((header) => ({
        name: header.name.trim(),
        value: header.value.trim(),
      }))
      .filter((header) => header.name || header.value);

    if (normalizedHeaders.some((header) => !header.name || !header.value)) {
      showToast('请完整填写每个 Header 的名称和值', 'error');
      return;
    }

    const headerNamePattern = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
    if (normalizedHeaders.some((header) => !headerNamePattern.test(header.name) || /[\r\n]/.test(header.value))) {
      showToast('Header 名称或值格式不合法', 'error');
      return;
    }

    const headerNames = normalizedHeaders.map((header) => header.name.toLowerCase());
    if (new Set(headerNames).size !== headerNames.length) {
      showToast('Header 名称不能重复（不区分大小写）', 'error');
      return;
    }

    const payload = {
      name: providerName,
      api_key: key,
      models,
      endpoints: normalizedEndpoints,
      headers: Object.fromEntries(normalizedHeaders.map((header) => [header.name, header.value])),
      status: editingProvider?.status || providerStatus,
      priority: parseInt(priority, 10) || 0,
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
        <div className="flex items-center justify-between">
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
              <TableHead>供应商名称</TableHead>
              <TableHead>Base URL</TableHead>
              <TableHead>支持协议</TableHead>
              <TableHead>优先级</TableHead>
              <TableHead>状态</TableHead>
              <TableHead>模型目录</TableHead>
              <TableHead>操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {providers.length > 0 ? (
              providers.map((provider) => {
                const isEnabled = provider.status === 'enabled';
                const endpointsList = Array.isArray(provider.endpoints) ? provider.endpoints : [];
                return (
                  <TableRow key={provider.name}>
                    <TableCell className="font-semibold">{provider.name}</TableCell>
                    <TableCell className="max-w-[260px]">
                      <div className="space-y-1">
                        {endpointsList.map((endpoint, index) => (
                          <div
                            key={`${provider.name}-${endpoint.protocol}-${index}`}
                            className="font-mono text-xs break-all"
                            title={endpoint.base_url}
                          >
                            {endpoint.base_url}
                          </div>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        {endpointsList.map((endpoint) => (
                          <Badge key={endpoint.protocol} variant="secondary" className="font-mono text-[10px]">
                            {endpoint.protocol}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell className="font-mono text-xs">
                      {provider.priority}
                    </TableCell>
                    <TableCell>
                      <Badge variant={isEnabled ? 'success' : 'destructive'}>
                        {isEnabled ? '启用' : '禁用'}
                      </Badge>
                    </TableCell>
                    <TableCell className="max-w-[260px]">
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
                <TableCell colSpan={7} className="py-8 text-center text-muted-foreground">暂无配置的供应商</TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </CardContent>

      <Dialog open={isModalOpen} onOpenChange={setIsModalOpen}>
        <DialogContent className="max-h-[90vh] max-w-[900px] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Layers size={18} className="text-primary" />
              <span>{editingProvider ? '编辑供应商' : '注册新供应商'}</span>
            </DialogTitle>
            <DialogDescription>
              配置共享 API Key、模型目录、上游端点和自定义请求 Header。
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={handleSaveProvider} className="space-y-5">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">供应商名称 *</Label>
                <Input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder="openai" required disabled={!!editingProvider} />
              </div>
              <div className="space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">API Key *</Label>
                <Input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="sk-..." required />
              </div>
            </div>

            <div className="grid grid-cols-12 gap-4 border-t border-border pt-4">
              <div className="col-span-12 space-y-3">
                <div className="flex items-center justify-between">
                  <Label className="text-xs uppercase tracking-wider text-muted-foreground">Endpoints *</Label>
                  <Button type="button" variant="outline" size="sm" onClick={addEndpointRow} className="gap-1 text-xs">
                    <Plus size={11} /> 添加 Endpoint
                  </Button>
                </div>
                <div className="space-y-2">
                  {endpoints.map((endpoint, index) => (
                    <div key={index} className="grid grid-cols-12 gap-2 rounded-lg border border-border bg-muted/50 p-3">
                      <div className="col-span-4 space-y-1">
                        <Label className="text-[10px] text-muted-foreground">协议</Label>
                        <Select
                          value={endpoint.protocol || ''}
                          onValueChange={(value) => handleEndpointChange(index, 'protocol', value)}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder="选择协议" />
                          </SelectTrigger>
                          <SelectContent>
                            {PROTOCOL_OPTIONS.map((proto) => {
                              const alreadyUsed = endpoints.some((item, itemIndex) => itemIndex !== index && item.protocol === proto);
                              return (
                                <SelectItem key={proto} value={proto} disabled={alreadyUsed}>
                                  {proto}
                                </SelectItem>
                              );
                            })}
                          </SelectContent>
                        </Select>
                      </div>
                      <div className="col-span-7 space-y-1">
                        <Label className="text-[10px] text-muted-foreground">Base URL</Label>
                        <Input
                          type="text"
                          value={endpoint.base_url || ''}
                          onChange={(e) => handleEndpointChange(index, 'base_url', e.target.value)}
                          placeholder="https://api.openai.com"
                        />
                      </div>
                      <div className="col-span-1 flex items-end">
                        <Button type="button" variant="destructive" size="icon" className="h-9 w-9" onClick={() => removeEndpointRow(index)}>
                          <Trash2 size={13} />
                        </Button>
                      </div>
                    </div>
                  ))}
                  {endpoints.length === 0 && (
                    <div className="rounded-lg border border-dashed border-border py-5 text-center text-xs text-muted-foreground">
                      点击"添加 Endpoint"开始配置
                    </div>
                  )}
                </div>
              </div>
              <div className="col-span-3 space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">优先级 (数字越小越优先)</Label>
                <Input type="number" value={priority} onChange={(e) => setPriority(e.target.value)} required />
              </div>
              <div className="col-span-3 space-y-2">
                <Label className="text-xs uppercase tracking-wider text-muted-foreground">状态</Label>
                <Select value={providerStatus} onValueChange={setProviderStatus}>
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

            <div className="border-t border-border pt-4">
              <div className="mb-3 flex items-center justify-between">
                <div>
                  <h4 className="text-sm font-semibold">自定义 Headers</h4>
                  <p className="mt-1 text-xs text-muted-foreground">
                    随供应商请求发送，可用于 Authorization、Anthropic Beta 等上游专用请求头。
                  </p>
                </div>
                <Button type="button" variant="outline" size="sm" onClick={addHeaderRow} className="gap-1 text-xs">
                  <Plus size={11} /> 添加 Header
                </Button>
              </div>

              <div className="space-y-2">
                {customHeaders.map((header, index) => (
                  <div key={index} className="grid grid-cols-12 gap-2 rounded-lg border border-border bg-muted/50 p-3">
                    <div className="col-span-4 space-y-1">
                      <Label className="text-[10px] text-muted-foreground">Header 名称</Label>
                      <Input
                        type="text"
                        value={header.name}
                        onChange={(event) => handleHeaderChange(index, 'name', event.target.value)}
                        placeholder="Authorization"
                        className="font-mono"
                      />
                    </div>
                    <div className="col-span-7 space-y-1">
                      <Label className="text-[10px] text-muted-foreground">Header 值</Label>
                      <Input
                        type="text"
                        value={header.value}
                        onChange={(event) => handleHeaderChange(index, 'value', event.target.value)}
                        placeholder="Bearer ..."
                        className="font-mono"
                      />
                    </div>
                    <div className="col-span-1 flex items-end">
                      <Button type="button" variant="destructive" size="icon" className="h-9 w-9" onClick={() => removeHeaderRow(index)}>
                        <Trash2 size={13} />
                      </Button>
                    </div>
                  </div>
                ))}
                {customHeaders.length === 0 && (
                  <div className="rounded-lg border border-dashed border-border py-5 text-center text-xs text-muted-foreground">
                    未配置自定义 Header
                  </div>
                )}
              </div>
            </div>

            <div className="border-t border-border pt-4">
              <div className="mb-3 flex items-center justify-between">
                <h4 className="text-sm font-semibold">模型 / 费率 / 别名</h4>
                <Button type="button" variant="outline" size="sm" onClick={addModelRow} className="gap-1 text-xs">
                  <Plus size={11} /> 添加模型
                </Button>
              </div>

              <div className="max-h-[300px] space-y-2 overflow-y-auto pr-1">
                {formModels.map((item, index) => (
                  <div key={index} className="flex items-end gap-2 rounded-lg border border-border bg-muted/50 p-3">
                    <div className="grid flex-1 grid-cols-12 gap-2">
                      <div className="col-span-4 space-y-1">
                        <Label className="text-[10px] text-muted-foreground">模型名称 *</Label>
                        <Input type="text" value={item.name} onChange={(e) => handleModelChange(index, 'name', e.target.value)} placeholder="gpt-4o-mini" required />
                      </div>
                      <div className="col-span-3 space-y-1">
                        <Label className="flex items-center gap-0.5 text-[10px] text-muted-foreground"><Shuffle size={9} /> 别名</Label>
                        <Input type="text" value={item.aliases} onChange={(e) => handleModelChange(index, 'aliases', e.target.value)} placeholder="gpt4,my-gpt" className="font-mono" />
                      </div>
                      <div className="col-span-2 space-y-1">
                        <Label className="flex items-center gap-0.5 text-[10px] text-muted-foreground"><Coins size={9} /> 输入/1K</Label>
                        <Input type="number" step="0.000001" value={item.inputPrice} onChange={(e) => handleModelChange(index, 'inputPrice', e.target.value)} placeholder="0.005" />
                      </div>
                      <div className="col-span-2 space-y-1">
                        <Label className="flex items-center gap-0.5 text-[10px] text-muted-foreground"><Coins size={9} /> 输出/1K</Label>
                        <Input type="number" step="0.000001" value={item.outputPrice} onChange={(e) => handleModelChange(index, 'outputPrice', e.target.value)} placeholder="0.015" />
                      </div>
                      <div className="col-span-1 space-y-1">
                        <Label className="text-[10px] text-muted-foreground">状态</Label>
                        <Select value={item.status} onValueChange={(value) => handleModelChange(index, 'status', value)}>
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
                    <Button type="button" variant="destructive" size="icon" className="h-8 w-8 shrink-0" onClick={() => removeModelRow(index)}>
                      <Trash2 size={13} />
                    </Button>
                  </div>
                ))}
                {formModels.length === 0 && (
                  <div className="rounded-lg border border-dashed border-border py-5 text-center text-xs text-muted-foreground">
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
