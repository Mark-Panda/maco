/** 去掉 plan 中的 checkbox 行，避免与待办时间线重复展示。 */
export function stripPlanChecklist(markdown: string): string {
  const kept = markdown.split("\n").filter((line) => {
    const t = line.trim();
    if (/^[-*+]\s*\[[\s xX~/-]?\]/.test(t)) return false;
    if (/^\d+\.\s*\[[\s xX~/-]?\]/.test(t)) return false;
    return true;
  });
  return kept.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/** 提取 plan 首个一级标题，用于侧栏摘要。 */
export function extractPlanTitle(markdown: string): string | null {
  for (const line of markdown.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("# ")) {
      const title = trimmed.slice(2).trim();
      return title || null;
    }
  }
  return null;
}
