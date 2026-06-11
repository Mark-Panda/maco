import { useState } from "react";
import {
  deleteModel,
  fetchModels,
  type ModelView,
  upsertModel,
} from "../api/client";

const PROVIDER_DEFAULTS = {
  openai: {
    model_id: "gpt-4o-mini",
    base_url: "https://api.openai.com/v1",
  },
  anthropic: {
    model_id: "claude-sonnet-4-6",
    base_url: "https://api.anthropic.com",
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
    provider: "openai" as "openai" | "anthropic",
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
      api_key_env: m.api_key_env,
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
    if (!confirm("Delete this model?")) return;
    await deleteModel(id);
    onChange(await fetchModels());
    if (editing === id) resetForm();
  }

  return (
    <div className="panel-section">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
        <h3 style={{ margin: 0 }}>Models</h3>
        <button type="button" className="btn btn-sm btn-primary" onClick={startNew}>
          + Add
        </button>
      </div>

      {models.length === 0 && !editing && (
        <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
          No models configured. Add one with your API key and base URL.
        </p>
      )}

      {models.map((m) => (
        <div key={m.id} className="model-list-item">
          <div>
            <strong>{m.name}</strong>
            {m.is_default && <span className="badge" style={{ marginLeft: 6 }}>default</span>}
            <div className="model-meta">
              {m.provider} · {m.model_id}
            </div>
            {m.base_url && <div className="model-meta">{m.base_url}</div>}
            <div className="model-meta">
              {m.has_api_key
                ? `Key: ${m.api_key_preview ?? "configured"}`
                : m.api_key_env
                  ? `Env: ${m.api_key_env}`
                  : "No API key"}
            </div>
          </div>
          <div style={{ display: "flex", gap: 4 }}>
            <button type="button" className="btn btn-sm" onClick={() => startEdit(m)}>
              Edit
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
            <label>Display name</label>
            <input
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="My GPT"
            />
          </div>
          <div className="field">
            <label>Provider</label>
            <select
              value={form.provider}
              onChange={(e) => {
                const p = e.target.value as "openai" | "anthropic";
                setForm({
                  ...form,
                  provider: p,
                  model_id: PROVIDER_DEFAULTS[p].model_id,
                  base_url: PROVIDER_DEFAULTS[p].base_url,
                });
              }}
            >
              <option value="openai">OpenAI / compatible</option>
              <option value="anthropic">Anthropic / compatible</option>
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
          <div className="field">
            <label>Base URL</label>
            <input
              value={form.base_url}
              onChange={(e) => setForm({ ...form, base_url: e.target.value })}
              placeholder="https://api.openai.com/v1"
            />
          </div>
          <div className="field">
            <label>API Key</label>
            <input
              type="password"
              value={form.api_key}
              onChange={(e) => setForm({ ...form, api_key: e.target.value })}
              placeholder={editing !== "new" ? "Leave blank to keep existing" : "sk-..."}
              autoComplete="off"
            />
          </div>
          <div className="field">
            <label>Or env var name (optional fallback)</label>
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
            Set as default model
          </label>
          {error && <p style={{ color: "#f87171", fontSize: "0.8rem" }}>{error}</p>}
          <div style={{ display: "flex", gap: 8 }}>
            <button type="button" className="btn btn-sm btn-primary" disabled={saving} onClick={save}>
              {saving ? "Saving…" : "Save"}
            </button>
            <button type="button" className="btn btn-sm" onClick={resetForm}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
