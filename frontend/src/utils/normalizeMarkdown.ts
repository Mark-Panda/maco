/** 规范化模型输出的 Markdown，修复常见 GFM 解析失败场景。 */
export function normalizeAssistantMarkdown(raw: string): string {
  let text = raw.replace(/\r\n/g, "\n").replace(/＃/g, "#").replace(/＊/g, "*").trim();
  if (!text) return text;

  text = fixFencedCodeBlocks(text);
  text = splitInlineTableLines(text);

  // 标题、加粗、代码块前补空行（表格用 ensureBlankLineBeforeTables，避免拆散 | a | b |）
  text = text.replace(/([^\n#])(#{1,6}\s)/g, "$1\n\n$2");
  text = text.replace(/([^\n*])(\*\*[^*\n]+\*\*)/g, "$1\n\n$2");
  text = text.replace(/([^\n`])(`{3,})/g, "$1\n\n$2");

  text = ensureBlankLineBeforeTables(text);
  text = fixTablesMissingHeader(text);
  text = ensureTableSeparators(text);
  text = dedupeTableSeparators(text);
  text = removeJunkTableRows(text);

  return text.replace(/\n{3,}/g, "\n\n").trim();
}

/** 将同一行内的 ```code``` 拆成独立 fenced block。 */
function fixFencedCodeBlocks(text: string): string {
  return text.replace(/`{3,}\s*([^\n`]+?)\s*`{3,}/g, (_, code) => {
    return `\n\n\`\`\`\n${code.trim()}\n\`\`\`\n\n`;
  });
}

/** 将挤在一行里的多行表格拆成多行。 */
function splitInlineTableLines(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed.includes("|")) {
      out.push(line);
      continue;
    }

    const parts = trimmed.split(/\s+(?=\|)/).map((p) => p.trim()).filter(Boolean);
    const tableParts = parts.filter(
      (p) =>
        p.length > 1 &&
        p.startsWith("|") &&
        (isTableRow(p) || isTableSeparator(p) || isPartialTableRow(p)),
    );

    if (tableParts.length <= 1) {
      out.push(line);
      continue;
    }

    for (const part of tableParts) {
      out.push(normalizeTableRowPadding(part));
    }
  }

  return out.join("\n");
}

function isPartialTableRow(line: string): boolean {
  return line.startsWith("|") && line.includes("|", 1);
}

/** 表格块前补空行，不破坏行内多列 `| a | b |`。 */
function ensureBlankLineBeforeTables(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (
      i > 0 &&
      (isTableRow(trimmed) || isTableSeparator(trimmed)) &&
      out.length > 0
    ) {
      const prev = out[out.length - 1]?.trim() ?? "";
      if (prev && !isTableRow(prev) && !isTableSeparator(prev)) {
        out.push("");
      }
    }
    out.push(lines[i]);
  }

  return out.join("\n");
}

function normalizeTableRowPadding(row: string): string {
  let r = row.trim();
  if (!r.endsWith("|")) r += " |";
  const cells = r
    .slice(1, r.lastIndexOf("|"))
    .split("|")
    .map((c) => c.trim());
  if (cells.length === 0) return row;
  return `| ${cells.join(" | ")} |`;
}

function ensureTableSeparators(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const trimmed = lines[i].trim();
    if (isTableRow(trimmed) && !isTableSeparator(trimmed)) {
      const block = [lines[i]];
      i += 1;
      while (i < lines.length) {
        const next = lines[i].trim();
        if (!isTableRow(next) && !isTableSeparator(next) && !isPartialTableRow(next)) break;
        if (isTableSeparator(next)) {
          block.push(lines[i]);
          i += 1;
          continue;
        }
        if (isJunkTableRow(next)) {
          i += 1;
          continue;
        }
        block.push(lines[i]);
        i += 1;
      }
      out.push(block[0]);
      if (block.length > 1 && !isTableSeparator(block[1]?.trim() ?? "")) {
        const cols = countTableColumns(block[0].trim());
        if (cols >= 2) out.push(buildTableSeparator(cols));
      }
      for (let j = 1; j < block.length; j++) out.push(block[j]);
      continue;
    }
    out.push(lines[i]);
    i += 1;
  }

  return out.join("\n");
}

function dedupeTableSeparators(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let inTable = false;
  let hasSeparator = false;

  for (const line of lines) {
    const t = line.trim();
    if (isTableRow(t) || isTableSeparator(t)) {
      if (!inTable) {
        inTable = true;
        hasSeparator = false;
      }
      if (isTableSeparator(t)) {
        if (hasSeparator) continue;
        hasSeparator = true;
      }
      out.push(line);
      continue;
    }
    inTable = false;
    hasSeparator = false;
    out.push(line);
  }

  return out.join("\n");
}

function removeJunkTableRows(text: string): string {
  const lines = text.split("\n");
  return lines
    .filter((line) => {
      const t = line.trim();
      if (!t.includes("|")) return true;
      if (isTableSeparator(t)) return true;
      if (isTableRow(t)) return !isJunkTableRow(t);
      return true;
    })
    .join("\n");
}

function isJunkTableRow(line: string): boolean {
  const t = line.trim();
  if (!t.startsWith("|")) return false;
  if (isTableSeparator(t)) return false;
  const cells = t
    .slice(1, t.endsWith("|") ? -1 : undefined)
    .split("|")
    .map((c) => c.trim());
  if (cells.length === 0) return true;
  return cells.every((c) => c.length === 0 || /^[-:\s]+$/.test(c));
}

function buildTableSeparator(cols: number): string {
  return `| ${Array.from({ length: cols }, () => "---").join(" | ")} |`;
}

function fixTablesMissingHeader(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (isTableSeparator(trimmed)) {
      const prev = out[out.length - 1]?.trim() ?? "";
      if (!isTableRow(prev)) {
        const cols = countTableColumns(trimmed);
        if (cols >= 2) {
          out.push(buildTableHeader(cols));
        }
      }
    }

    out.push(line);
  }

  return out.join("\n");
}

function isTableRow(line: string): boolean {
  const t = line.trim();
  return t.startsWith("|") && t.endsWith("|") && !isTableSeparator(t);
}

function isTableSeparator(line: string): boolean {
  const t = line.trim();
  if (!/^\|.+\|$/.test(t)) return false;
  return t
    .slice(1, -1)
    .split("|")
    .every((cell) => /^[\s\-:|]+$/.test(cell.trim()));
}

function countTableColumns(line: string): number {
  const t = line.trim();
  if (!t.startsWith("|")) return 0;
  const inner = t.endsWith("|") ? t.slice(1, -1) : t.slice(1);
  return inner.split("|").filter((cell) => cell.trim().length > 0).length;
}

function buildTableHeader(cols: number): string {
  const cells = Array.from({ length: cols }, (_, i) => (i === 0 ? "功能" : "说明"));
  return `| ${cells.join(" | ")} |`;
}
