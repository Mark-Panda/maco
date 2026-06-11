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
      <p className="panel-empty" style={{ paddingTop: 0 }}>
        扫描目录 <code>~/.maco/skills/**/*.md</code>
      </p>
      {skills.length === 0 ? (
        <p className="panel-empty">未找到技能文件</p>
      ) : (
        skills.map((s) => (
          <button
            key={s.name}
            type="button"
            className={`panel-card panel-card--clickable ${selected === s.name ? "active" : ""}`}
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
            <p className="panel-empty">加载中…</p>
          ) : (
            <pre className="panel-pre">
              {content || "技能文件为空"}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}
