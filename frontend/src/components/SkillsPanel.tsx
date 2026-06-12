import { useCallback, useEffect, useRef, useState } from "react";

import {
  deleteSkill,
  fetchSkill,
  fetchSkills,
  updateSkillEnabled,
  uploadSkillZip,
  type SkillSummary,
} from "../api/client";
import { useConfirmDialog } from "../hooks/useConfirmDialog";
import { MacoIcon } from "./Icons";

function formatUpdatedAt(iso?: string | null): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function SkillsPanel() {
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [listLoading, setListLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [overwrite, setOverwrite] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [togglingName, setTogglingName] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const enabledCount = skills.filter((s) => s.enabled).length;

  const reloadSkills = useCallback(async () => {
    setListLoading(true);
    try {
      const rows = await fetchSkills();
      setSkills(rows);
      setError(null);
    } catch (e) {
      setSkills([]);
      setError(String(e));
    } finally {
      setListLoading(false);
    }
  }, []);

  useEffect(() => {
    void reloadSkills();
  }, [reloadSkills]);

  async function openSkill(name: string) {
    setSelected(name);
    setLoading(true);
    setError(null);
    try {
      const detail = await fetchSkill(name);
      setContent(detail.content);
    } catch (e) {
      setContent("");
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function onUpload(file: File) {
    setUploading(true);
    setMessage(null);
    setError(null);
    try {
      const result = await uploadSkillZip(file, overwrite);
      setMessage(
        `已安装「${result.name}」，解压 ${result.extracted_files} 个文件。`,
      );
      await reloadSkills();
      await openSkill(result.name);
    } catch (e) {
      setError(String(e));
    } finally {
      setUploading(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  }

  async function onToggleEnabled(skill: SkillSummary, enabled: boolean) {
    setTogglingName(skill.name);
    setError(null);
    try {
      const updated = await updateSkillEnabled(skill.name, enabled);
      setSkills((rows) =>
        rows.map((row) => (row.name === updated.name ? { ...row, ...updated } : row)),
      );
      setMessage(enabled ? `已启用「${skill.name}」` : `已禁用「${skill.name}」`);
    } catch (e) {
      setError(String(e));
    } finally {
      setTogglingName(null);
    }
  }

  const { runConfirm, dialog } = useConfirmDialog();

  function onDelete(name: string) {
    runConfirm(
      {
        title: "删除 Skill",
        description: `确定删除 Skill「${name}」？此操作不可恢复。`,
        confirmLabel: "删除",
        tone: "danger",
      },
      async () => {
        setError(null);
        setMessage(null);
        try {
          await deleteSkill(name);
          if (selected === name) {
            setSelected(null);
            setContent("");
          }
          setMessage(`已删除「${name}」。`);
          await reloadSkills();
        } catch (e) {
          setError(String(e));
        }
      },
    );
  }

  return (
    <>
    <div className="skills-panel">
      <div className="skills-panel-toolbar">
        <div className="skills-upload">
          <input
            ref={fileInputRef}
            type="file"
            accept=".zip,application/zip"
            hidden
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void onUpload(file);
            }}
          />
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={uploading}
            onClick={() => fileInputRef.current?.click()}
          >
            {uploading ? "安装中…" : "上传 Skill zip"}
          </button>
          <label className="skills-upload-overwrite">
            <input
              type="checkbox"
              checked={overwrite}
              onChange={(e) => setOverwrite(e.target.checked)}
            />
            覆盖同名 Skill
          </label>
        </div>
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() => void reloadSkills()}
          disabled={listLoading}
        >
          刷新
        </button>
      </div>

      <p className="panel-empty skills-panel-hint">
        遵循 ADK/agentskills 规范：zip 内推荐 <code>skill-name/SKILL.md</code>（含{" "}
        <code>name</code> / <code>description</code> frontmatter）。发现路径：项目{" "}
        <code>.skills/</code>、<code>.claude/skills/</code>、全局 <code>~/.maco/skills/</code>
        。禁用的 Skill 不会注入 Agent；含 <code>allowed-tools</code> 的 Skill 仅绑定声明的工具。
      </p>

      {message ? <p className="skills-panel-message">{message}</p> : null}
      {error ? <p className="skills-panel-error">{error}</p> : null}

      <div className="skills-panel-body">
        <div className="skills-list-card panel-card">
          <div className="skills-list-header">
            <h3>全部 Skill</h3>
            <span className="panel-count">
              {enabledCount}/{skills.length} 启用
            </span>
          </div>
          {listLoading ? (
            <p className="panel-empty">加载中…</p>
          ) : skills.length === 0 ? (
            <p className="panel-empty">还没有 Skill，请上传 zip 或放入本地目录后刷新。</p>
          ) : (
            <div className="skills-list">
              {skills.map((s) => {
                const active = selected === s.name;
                return (
                  <div
                    key={s.name}
                    className={`skills-list-item${active ? " active" : ""}${s.enabled ? "" : " skills-list-item--disabled"}`}
                  >
                    <label
                      className="skills-list-toggle"
                      title={s.enabled ? "点击禁用" : "点击启用"}
                      onClick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={s.enabled}
                        disabled={togglingName === s.name}
                        onChange={(e) => void onToggleEnabled(s, e.target.checked)}
                      />
                    </label>
                    <button
                      type="button"
                      className="skills-list-item-main"
                      onClick={() => void openSkill(s.name)}
                    >
                      <span className="skills-list-item-title">
                        {s.name}
                        {!s.enabled ? (
                          <span className="skills-list-badge">已禁用</span>
                        ) : null}
                      </span>
                      {s.description ? (
                        <span className="skills-list-item-desc">{s.description}</span>
                      ) : null}
                      <span className="skills-list-item-meta">
                        {formatUpdatedAt(s.updated_at)}
                        {s.updated_at ? " · " : ""}
                        {s.file_path}
                      </span>
                    </button>
                    <button
                      type="button"
                      className="skills-list-delete"
                      title="删除"
                      aria-label={`删除 ${s.name}`}
                      onClick={() => void onDelete(s.name)}
                    >
                      <MacoIcon name="x" size={16} />
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="skills-detail-card panel-card">
          {selected ? (
            <>
              <div className="skills-detail-header">
                <h3>{selected}</h3>
              </div>
              {loading ? (
                <p className="panel-empty">加载中…</p>
              ) : (
                <pre className="panel-pre skills-detail-pre">
                  {content || "技能文件为空"}
                </pre>
              )}
            </>
          ) : (
            <div className="skills-detail-empty">
              <MacoIcon name="skills" size={28} />
              <p>选择左侧 Skill 查看正文</p>
            </div>
          )}
        </div>
      </div>
    </div>
    {dialog}
    </>
  );
}
