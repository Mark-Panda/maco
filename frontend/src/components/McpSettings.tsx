import { useEffect, useState } from "react";
import {
  createMcpServer,
  deleteMcpServer,
  friendlyWorktreeError,
  listMcpServers,
  type McpServerRecord,
  reloadMcpPool,
  updateMcpServer,
} from "../api/client";
import { useConfirmDialog } from "../hooks/useConfirmDialog";

type FormState = {
  name: string;
  transport: "stdio" | "sse";
  command: string;
  args: string;
  url: string;
  env: string;
  enabled: boolean;
};

const EMPTY_FORM: FormState = {
  name: "",
  transport: "stdio",
  command: "",
  args: "[]",
  url: "",
  env: "{}",
  enabled: true,
};

export function McpSettings() {
  const [servers, setServers] = useState<McpServerRecord[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [reloading, setReloading] = useState(false);

  async function refresh() {
    setServers(await listMcpServers());
  }

  useEffect(() => {
    refresh().catch(() => setServers([]));
  }, []);

  function resetForm() {
    setEditing(null);
    setForm(EMPTY_FORM);
    setError("");
  }

  function startNew() {
    resetForm();
    setEditing("new");
  }

  function startEdit(s: McpServerRecord) {
    setEditing(s.id);
    setForm({
      name: s.name,
      transport: s.transport,
      command: s.command ?? "",
      args: s.args,
      url: s.url ?? "",
      env: s.env,
      enabled: s.enabled !== 0,
    });
    setError("");
  }

  async function save() {
    setSaving(true);
    setError("");
    try {
      JSON.parse(form.env);
      const isBuiltinFilesystem = form.name.trim() === "filesystem";
      if (!isBuiltinFilesystem) {
        JSON.parse(form.args);
      }
      const body = isBuiltinFilesystem
        ? {
            name: "filesystem",
            transport: "stdio",
            env: form.env,
            enabled: form.enabled,
          }
        : {
            name: form.name.trim(),
            transport: form.transport,
            command: form.transport === "stdio" ? form.command.trim() : undefined,
            args: form.args,
            url: form.transport === "sse" ? form.url.trim() : undefined,
            env: form.env,
            enabled: form.enabled,
          };
      if (editing === "new") {
        await createMcpServer(body);
      } else if (editing) {
        await updateMcpServer(editing, body);
      }
      await refresh();
      resetForm();
    } catch (e) {
      setError(friendlyWorktreeError(String(e)));
    } finally {
      setSaving(false);
    }
  }

  const { runConfirm, dialog } = useConfirmDialog();

  function remove(id: string) {
    runConfirm(
      {
        title: "删除 MCP 服务",
        description: "确定删除该 MCP 服务？",
        confirmLabel: "删除",
        tone: "danger",
      },
      async () => {
        await deleteMcpServer(id);
        await refresh();
        if (editing === id) resetForm();
      },
    );
  }

  async function reload() {
    setReloading(true);
    setError("");
    try {
      await reloadMcpPool();
      await refresh();
    } catch (e) {
      setError(friendlyWorktreeError(String(e)));
    } finally {
      setReloading(false);
    }
  }

  const mcpReloadHint =
    "重载仅更新全局 MCP 连接池（自定义 stdio/SSE）；不影响各会话正在运行的 filesystem 子进程。活跃 Run 期间重载会被拒绝。";

  const editingBuiltinFilesystem = Boolean(editing) && form.name === "filesystem";

  return (
    <>
    <div className="panel-section">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
        <h3 style={{ margin: 0 }}>MCP 服务</h3>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            type="button"
            className="btn btn-sm"
            onClick={reload}
            disabled={reloading}
            title={mcpReloadHint}
          >
            {reloading ? "…" : "重载"}
          </button>
          <button type="button" className="btn btn-sm btn-primary" onClick={startNew}>
            + 添加
          </button>
        </div>
      </div>

      <p style={{ fontSize: "0.8rem", color: "var(--text-muted)", marginBottom: 10 }}>
        内置 filesystem MCP 由 maco 按会话启动独立子进程，根目录为 scratch + 工作区；worktree
        模式下 MCP 工具路径参数也会拦截主仓库访问（可在「工具策略」中关闭）。自定义 MCP 可在此添加。
        {mcpReloadHint}
      </p>

      {servers.length === 0 && !editing && (
        <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>
          暂无 MCP 服务，可添加 stdio 或 SSE 端点供 Agent 调用工具。
        </p>
      )}

      {servers.map((s) => {
        const isBuiltinFilesystem = s.name === "filesystem";
        return (
        <div key={s.id} className="model-list-item">
          <div>
            <strong>{s.name}</strong>
            <span className="badge" style={{ marginLeft: 6 }}>
              {s.transport}
            </span>
            {isBuiltinFilesystem && (
              <span className="badge" style={{ marginLeft: 4 }}>
                内置
              </span>
            )}
            {!s.enabled && (
              <span className="badge" style={{ marginLeft: 4, opacity: 0.6 }}>
                已禁用
              </span>
            )}
            <div className="model-meta">
              {isBuiltinFilesystem
                ? "每轮 Run 按会话工作区动态启动；args 由 maco 管理"
                : s.transport === "stdio"
                  ? `${s.command ?? ""} ${s.args}`
                  : s.url ?? ""}
            </div>
          </div>
          <div style={{ display: "flex", gap: 4 }}>
            <button type="button" className="btn btn-sm" onClick={() => startEdit(s)}>
              编辑
            </button>
            <button
              type="button"
              className="btn btn-sm btn-ghost"
              onClick={() => remove(s.id)}
              disabled={isBuiltinFilesystem}
              title={isBuiltinFilesystem ? "内置服务不可删除" : undefined}
            >
              ×
            </button>
          </div>
        </div>
        );
      })}

      {editing && (
        <div className="panel-card" style={{ marginTop: 12 }}>
          {editingBuiltinFilesystem && (
            <p style={{ fontSize: "0.8rem", color: "var(--text-muted)", marginBottom: 10 }}>
              内置 filesystem：仅可调整启用状态与环境变量；允许目录在每轮 Run 中由 maco 自动设置。
            </p>
          )}
          <div className="field">
            <label>名称</label>
            <input
              value={form.name}
              disabled={editingBuiltinFilesystem}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder="filesystem"
            />
          </div>
          <div className="field">
            <label>传输方式</label>
            <select
              value={form.transport}
              disabled={editingBuiltinFilesystem}
              onChange={(e) =>
                setForm({ ...form, transport: e.target.value as "stdio" | "sse" })
              }
            >
              <option value="stdio">stdio</option>
              <option value="sse">sse</option>
            </select>
          </div>
          {form.transport === "stdio" ? (
            <>
              <div className="field">
                <label>命令</label>
                <input
                  value={form.command}
                  disabled={editingBuiltinFilesystem}
                  onChange={(e) => setForm({ ...form, command: e.target.value })}
                  placeholder="npx"
                />
              </div>
              <div className="field">
                <label>参数（JSON 数组）</label>
                <textarea
                  className="chat-input"
                  style={{ width: "100%", minHeight: 56 }}
                  value={form.args}
                  disabled={editingBuiltinFilesystem}
                  onChange={(e) => setForm({ ...form, args: e.target.value })}
                />
              </div>
            </>
          ) : (
            <div className="field">
              <label>SSE URL</label>
              <input
                value={form.url}
                onChange={(e) => setForm({ ...form, url: e.target.value })}
                placeholder="http://127.0.0.1:3001/sse"
              />
            </div>
          )}
          <div className="field">
            <label>环境变量（JSON 对象）</label>
            <textarea
              className="chat-input"
              style={{ width: "100%", minHeight: 48 }}
              value={form.env}
              onChange={(e) => setForm({ ...form, env: e.target.value })}
            />
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
            <button type="button" className="btn btn-sm" onClick={resetForm}>
              取消
            </button>
          </div>
        </div>
      )}
    </div>
    {dialog}
    </>
  );
}
