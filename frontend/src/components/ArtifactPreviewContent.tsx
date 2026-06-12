import { useEffect, useState } from "react";

import {
  fetchArtifactBlob,
  previewArtifact,
  type ArtifactRecord,
} from "../api/client";

type Props = {
  sessionId: string;
  artifact: Pick<ArtifactRecord, "id" | "filename" | "mime_type" | "size_bytes">;
};

export function ArtifactPreviewContent({ sessionId, artifact }: Props) {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [kind, setKind] = useState<"text" | "image" | "binary">("binary");

  useEffect(() => {
    let objectUrl: string | null = null;
    let cancelled = false;
    setLoading(true);
    setError(null);
    setText(null);
    setImageUrl(null);
    setTruncated(false);
    setKind("binary");

    (async () => {
      try {
        const preview = await previewArtifact(sessionId, artifact.id);
        if (cancelled) return;
        setKind(preview.kind);
        setTruncated(preview.truncated);
        if (preview.kind === "text" && preview.content != null) {
          setText(preview.content);
        } else if (preview.kind === "image") {
          const blob = await fetchArtifactBlob(sessionId, artifact.id);
          if (cancelled) return;
          objectUrl = URL.createObjectURL(blob);
          setImageUrl(objectUrl);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [sessionId, artifact.id]);

  if (loading) {
    return <p className="artifact-preview-loading">加载预览…</p>;
  }

  if (error) {
    return <p className="panel-empty">{error}</p>;
  }

  if (kind === "text" && text != null) {
    return (
      <>
        {truncated ? (
          <p className="artifact-preview-hint">内容过长，仅显示前 512 KB</p>
        ) : null}
        <pre className="artifact-preview-pre artifact-preview-pre--inline">{text}</pre>
      </>
    );
  }

  if (kind === "image" && imageUrl) {
    return (
      <img className="artifact-preview-image artifact-preview-image--inline" src={imageUrl} alt={artifact.filename} />
    );
  }

  return <p className="panel-empty">该文件类型不支持内联预览，请下载查看。</p>;
}
