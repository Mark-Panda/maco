import { useEffect, useState } from "react";

import { useConfirmDialog } from "../hooks/useConfirmDialog";
import {
  createToolPolicy,
  deleteToolPolicy,
  fetchWorktreePathGuard,
  listToolPolicies,
  reloadToolPolicies,
  type ToolPolicyRecord,
  updateToolPolicy,
  updateWorktreePathGuard,
} from "../api/client";

type FormState = {
  tool_pattern: string;
  source_type: string;
  action: string;
  enabled: boolean;
};

const EMPTY: FormState = {
  tool_pattern: "",
  source_type: "mcp",
  action: "confirm",
  enabled: true,
};

const ACTION_LABELS: Record<string, string> = {
  allow: "放行",
  confirm: "需确认",
  deny: "拒绝",
};

export function ToolPolicySettings() {
  const [policies, setPolicies] = useState<ToolPolicyRecord[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [worktreePathGuard, setWorktreePathGuard] = useState(true);
  const [guardSaving, setGuardSaving] = useState(false);

  async function refresh() {
    setPolicies(await listToolPolicies());
    const guard = await fetchWorktreePathGuard();
    setWorktreePathGuard(guard.enabled);
  }

  useEffect(() => {
    refresh().catch(() => setPolicies([]));
  }, []);

  async function toggleWorktreePathGuard(enabled: boolean) {
    setGuardSaving(true);
    setError("");
    try {
      const res = await updateWorktreePathGuard(enabled);
      setWorktreePathGuard(res.enabled);
    } catch (e) {
      setError(String(e));
    } finally {
      setGuardSaving(false);
    }
  }

  function resetForm() {
    setEditing(null);
    setForm(EMPTY);
    setError("");
  }

  function startNew() {
    resetForm();
    setEditing("new");
  }

  function startEdit(p: ToolPolicyRecord) {
    setEditing(p.id);
    setForm({
      tool_pattern: p.tool_pattern,
      source_type: p.source_type,
      action: p.action,
      enabled: p.enabled !== 0,
    });
    setError("");
  }

  async function save() {
    setSaving(true);
    setError("");
    try {
      const body = {
        tool_pattern: form.tool_pattern.trim(),
        source_type: form.source_type.trim(),
        action: form.action.trim(),
        enabled: form.enabled,
      };
      if (editing === "new") {
        await createToolPolicy(body);
      } else if (editing) {
        await updateToolPolicy(editing, body);
      }
      await refresh();
      resetForm();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const { runConfirm, dialog } = useConfirmDialog();

  function remove(id: string) {
    runConfirm(
      {
        title: "删除工具策略",
        description: "确定删除该策略？",
        confirmLabel: "删除",
        tone: "danger",
      },
      async () => {
        await deleteToolPolicy(id);
        await refresh();
        if (editing === id) resetForm();
      },
    );
  }

  return (
    <>
    <div className="panel-section">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
        <h3 style={{ margin: 0 }}>工具策略（HITL）</h3>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => reloadToolPolicies().then(refresh).catch((e) => setError(String(e)))}
          >
            重载
          </button>
          <button type="button" className="btn btn-sm btn-primary" onClick={startNew}>
            + 添加
          </button>
        </div>
      </div>
      <p style={{ fontSize: "0.85rem", color: "var(--text-muted)", marginTop: 0 }}>
        工具名支持 <code>*</code> 通配；<code>confirm</code> 会暂停并等待你确认。
      </p>

      <label
        className="panel-card"
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: 10,
          marginBottom: 12,
          padding: 12,
          fontSize: "0.85rem",
        }}
      >
        <input
          type="checkbox"
          checked={worktreePathGuard}
          disabled={guardSaving}
          onChange={(e) => void toggleWorktreePathGuard(e.target.checked)}
        />
        <span>
          <strong>Worktree 路径强制</strong>
          <br />
          <span style={{ color: "var(--text-muted)" }}>
            启用时，worktree 模式下拦截 bash 与 MCP 工具访问主仓库路径（仅允许 worktree 目录）。
            关闭后仅依赖 Agent 提示，不自动拦截。
          </span>
        </span>
      </label>

      {policies.map((p) => (
        <div key={p.id} className="model-list-item">
          <div>
            <strong>{p.tool_pattern}</strong>
            <span className="badge" style={{ marginLeft: 6 }}>
              {ACTION_LABELS[p.action] ?? p.action}
            </span>
            {!p.enabled && <span className="badge" style={{ marginLeft: 4, opacity: 0.6 }}>已禁用</span>}
            <div className="model-meta">来源: {p.source_type}</div>
          </div>
          <div style={{ display: "flex", gap: 4 }}>
            <button type="button" className="btn btn-sm" onClick={() => startEdit(p)}>编辑</button>
            <button type="button" className="btn btn-sm btn-ghost" onClick={() => remove(p.id)}>×</button>
          </div>
        </div>
      ))}

      {editing && (
        <div className="panel-card" style={{ marginTop: 12 }}>
          <div className="field">
            <label>工具名模式</label>
            <input
              value={form.tool_pattern}
              onChange={(e) => setForm({ ...form, tool_pattern: e.target.value })}
              placeholder="delete_*"
            />
          </div>
          <div className="field">
            <label>来源类型</label>
            <select
              value={form.source_type}
              onChange={(e) => setForm({ ...form, source_type: e.target.value })}
            >
              <option value="mcp">mcp</option>
              <option value="tool">tool</option>
              <option value="builtin">builtin</option>
            </select>
          </div>
          <div className="field">
            <label>动作</label>
            <select
              value={form.action}
              onChange={(e) => setForm({ ...form, action: e.target.value })}
            >
              <option value="allow">allow — 放行</option>
              <option value="confirm">confirm — 需确认</option>
              <option value="deny">deny — 拒绝</option>
            </select>
          </div>
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "0.85rem", marginBottom: 12 }}>
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(e) => setForm({ ...form, enabled: e.target.checked })}
            />
            启用
          </label>
          {error && <p style={{ color: "#f87171", fontSize: "0.8rem" }}>{error}</p>}
          <div style={{ display: "flex", gap: 8 }}>
            <button type="button" className="btn btn-sm btn-primary" disabled={saving} onClick={save}>
              {saving ? "保存中…" : "保存"}
            </button>
            <button type="button" className="btn btn-sm" onClick={resetForm}>取消</button>
          </div>
        </div>
      )}
    </div>
    {dialog}
    </>
  );
}
