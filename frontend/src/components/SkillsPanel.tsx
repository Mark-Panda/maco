import { useEffect, useState } from "react";
import { fetchSkill, fetchSkills, type SkillSummary } from "../api/client";

export function SkillsPanel() {
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    fetchSkills()
      .then(setSkills)
      .catch(() => setSkills([]));
  }, []);

  async function openSkill(name: string) {
    setSelected(name);
    setLoading(true);
    try {
      const detail = await fetchSkill(name);
      setContent(detail.content);
    } catch {
      setContent("");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="panel-section">
      <h3>技能</h3>
      <p style={{ fontSize: "0.85rem", color: "var(--text-muted)", marginTop: 0 }}>
        扫描目录 <code>~/.maco/skills/**/*.md</code>
      </p>
      {skills.length === 0 ? (
        <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>未找到技能文件</p>
      ) : (
        skills.map((s) => (
          <button
            key={s.name}
            type="button"
            className={`panel-card ${selected === s.name ? "active" : ""}`}
            style={{ width: "100%", textAlign: "left", marginBottom: 6, cursor: "pointer" }}
            onClick={() => openSkill(s.name)}
          >
            <strong>{s.name}</strong>
            <div className="model-meta">{s.file_path}</div>
          </button>
        ))
      )}
      {selected && (
        <div className="panel-card" style={{ marginTop: 10 }}>
          <strong>{selected}</strong>
          {loading ? (
            <p style={{ fontSize: "0.85rem", color: "var(--text-muted)" }}>加载中…</p>
          ) : (
            <pre className="panel-pre" style={{ maxHeight: 280, overflow: "auto" }}>
              {content || "技能文件为空"}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}
