import type { ReactNode } from "react";
import remarkGfm from "remark-gfm";

export const MARKDOWN_REMARK_PLUGINS = [remarkGfm];

function MarkdownTable(props: React.ComponentProps<"table"> & { children?: ReactNode }) {
  const { children, ...rest } = props;
  return (
    <div className="markdown-table-wrap">
      <table {...rest}>{children}</table>
    </div>
  );
}

export const MARKDOWN_COMPONENTS = {
  table: MarkdownTable,
};
