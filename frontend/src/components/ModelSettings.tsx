import { useState } from "react";
import {
  deleteModel,
  fetchModels,
  type ModelView,
  upsertModel,
} from "../api/client";

type Provider = ModelView["provider"];

const PROVIDER_DEFAULTS: Record<
  Provider,
  { model_id: string; base_url: string; api_key_env: string }
> = {
  openai: {
    model_id: "gpt-4o-mini",
    base_url: "https://api.openai.com/v1",
    api_key_env: "OPENAI_API_KEY",
  },
  anthropic: {
    model_id: "claude-sonnet-4-6",
    base_url: "https://api.anthropic.com",
    api_key_env: "ANTHROPIC_API_KEY",
  },
  gemini: {
    model_id: "gemini-2.5-flash",
    base_url: "",
    api_key_env: "GOOGLE_API_KEY",
  },
  openrouter: {
    model_id: "google/gemini-2.5-flash",
    base_url: "https://openrouter.ai/api/v1",
    api_key_env: "OPENROUTER_API_KEY",
  },
};

type Props = {
  models: ModelView[];
  onChange: (models: ModelView[]) => void;
};

export function ModelSettings({ models, onChange }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  const [form, setForm] = useState({
    name: "",
    provider: "openai" as Provider,
    model_id: "",
    base_url: "",
    api_key: "",
    api_key_env: "",
    is_default: false,
  });
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  function resetForm() {
    setEditing(null);
    setForm({
      name: "",
      provider: "openai",
      model_id: PROVIDER_DEFAULTS.openai.model_id,
      base_url: PROVIDER_DEFAULTS.openai.base_url,
      api_key: "",
      api_key_env: "",
      is_default: models.length === 0,
    });
    setError("");
  }

  function startNew() {
    resetForm();
    setEditing("new");
  }

  function startEdit(m: ModelView) {
    setEditing(m.id);
    setForm({
      name: m.name,
      provider: m.provider,
      model_id: m.model_id,
      base_url: m.base_url ?? PROVIDER_DEFAULTS[m.provider].base_url,
      api_key: "",
      api_key_env: m.api_key_env || PROVIDER_DEFAULTS[m.provider].api_key_env,
      is_default: m.is_default,
    });
    setError("");
  }

  async function save() {
    setSaving(true);
    setError("");
    try {
      const body = {
        name: form.name,
        provider: form.provider,
        model_id: form.model_id,
        base_url: form.base_url || undefined,
        api_key: form.api_key || undefined,
        api_key_env: form.api_key_env || undefined,
        is_default: form.is_default,
        enabled: true,
      };
      if (editing === "new") {
        await upsertModel(body);
      } else if (editing) {
        await upsertModel(body, editing);
      }
      onChange(await fetchModels());
      resetForm();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function remove(id: string) {
    if (!confirm("确定删除该模型配置？")) return;
    await deleteModel(id);
    onChange(await fetchModels());
    if (editing === id) resetForm();
  }

  return (
    <div className="panel-section">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
        <h3 style={{ margin: 0 }}>模型</h3>
        <button type="button" className="btn btn-sm btn-primary" onClick={startNew}>
          + 添加
        </button>
      </div>

      {models.length === 0 && !editing && (
        <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
          尚未配置模型，请添加 API Key 与 Base URL。
        </p>
      )}

      {models.map((m) => (
        <div key={m.id} className="model-list-item">
          <div>
            <strong>{m.name}</strong>
            {m.is_default && <span className="badge" style={{ marginLeft: 6 }}>默认</span>}
            <div className="model-meta">
              {m.provider} · {m.model_id}
            </div>
            {m.base_url && <div className="model-meta">{m.base_url}</div>}
            <div className="model-meta">
              {m.has_api_key
                ? `密钥: ${m.api_key_preview ?? "已配置"}`
                : m.api_key_env
                  ? `环境变量: ${m.api_key_env}`
                  : "未配置 API Key"}
            </div>
          </div>
          <div style={{ display: "flex", gap: 4 }}>
            <button type="button" className="btn btn-sm" onClick={() => startEdit(m)}>
              编辑
            </button>
            <button type="button" className="btn btn-sm btn-ghost" onClick={() => remove(m.id)}>
              ×
            </button>
          </div>
        </div>
      ))}

      {editing && (
        <div className="panel-card" style={{ marginTop: 12 }}>
          <div className="field">
            <label>显示名称</label>
            <input
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="我的 GPT"
            />
          </div>
          <div className="field">
            <label>提供商</label>
            <select
              value={form.provider}
              onChange={(e) => {
                const p = e.target.value as Provider;
                setForm({
                  ...form,
                  provider: p,
                  model_id: PROVIDER_DEFAULTS[p].model_id,
                  base_url: PROVIDER_DEFAULTS[p].base_url,
                  api_key_env: PROVIDER_DEFAULTS[p].api_key_env,
                });
              }}
            >
              <option value="openai">OpenAI / 兼容</option>
              <option value="anthropic">Anthropic / 兼容</option>
              <option value="gemini">Google Gemini</option>
              <option value="openrouter">OpenRouter</option>
            </select>
          </div>
          <div className="field">
            <label>Model ID</label>
            <input
              value={form.model_id}
              onChange={(e) => setForm({ ...form, model_id: e.target.value })}
              placeholder="gpt-4o-mini"
            />
          </div>
          {form.provider !== "gemini" && (
            <div className="field">
              <label>Base URL</label>
              <input
                value={form.base_url}
                onChange={(e) => setForm({ ...form, base_url: e.target.value })}
                placeholder={PROVIDER_DEFAULTS[form.provider].base_url}
              />
            </div>
          )}
          <div className="field">
            <label>API Key</label>
            <input
              type="password"
              value={form.api_key}
              onChange={(e) => setForm({ ...form, api_key: e.target.value })}
              placeholder={editing !== "new" ? "留空则保持原密钥" : "sk-..."}
              autoComplete="off"
            />
          </div>
          <div className="field">
            <label>或环境变量名（可选兜底）</label>
            <input
              value={form.api_key_env}
              onChange={(e) => setForm({ ...form, api_key_env: e.target.value })}
              placeholder="OPENAI_API_KEY"
            />
          </div>
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "0.85rem", marginBottom: 12 }}>
            <input
              type="checkbox"
              checked={form.is_default}
              onChange={(e) => setForm({ ...form, is_default: e.target.checked })}
            />
            设为默认模型
          </label>
          {error && <p style={{ color: "#f87171", fontSize: "0.8rem" }}>{error}</p>}
          <div style={{ display: "flex", gap: 8 }}>
            <button type="button" className="btn btn-sm btn-primary" disabled={saving} onClick={save}>
              {saving ? "保存中…" : "保存"}
            </button>
            <button type="button" className="btn btn-sm" onClick={resetForm}>
              取消
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
