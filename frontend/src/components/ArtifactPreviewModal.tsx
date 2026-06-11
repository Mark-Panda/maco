import { useEffect, useState } from "react";
import {
  downloadArtifact,
  fetchArtifactBlob,
  previewArtifact,
  type ArtifactRecord,
} from "../api/client";

type Props = {
  sessionId: string;
  artifact: Pick<ArtifactRecord, "id" | "filename" | "mime_type" | "size_bytes">;
  onClose: () => void;
};

export function ArtifactPreviewModal({ sessionId, artifact, onClose }: Props) {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [kind, setKind] = useState<"text" | "image" | "binary">("binary");

  useEffect(() => {
    let objectUrl: string | null = null;
    let cancelled = false;

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

  return (
    <div className="artifact-modal-overlay" onClick={onClose} role="presentation">
      <div
        className="artifact-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="artifact-modal-title"
      >
        <header className="artifact-modal-header">
          <div>
            <h2 id="artifact-modal-title">{artifact.filename}</h2>
            <p className="artifact-modal-meta">
              {artifact.mime_type} · {(artifact.size_bytes / 1024).toFixed(1)} KB
            </p>
          </div>
          <div className="artifact-modal-actions">
            <button
              type="button"
              className="btn btn-sm"
              onClick={() =>
                downloadArtifact(sessionId, artifact.id, artifact.filename).catch((err) =>
                  setError(String(err)),
                )
              }
            >
              下载
            </button>
            <button type="button" className="btn btn-sm btn-ghost" onClick={onClose}>
              关闭
            </button>
          </div>
        </header>
        <div className="artifact-modal-body">
          {loading && <p className="panel-empty">加载中…</p>}
          {error && <p className="panel-empty">{error}</p>}
          {!loading && !error && kind === "text" && text != null && (
            <>
              {truncated && (
                <p className="artifact-modal-hint">内容过长，仅显示前 512 KB</p>
              )}
              <pre className="artifact-preview-pre">{text}</pre>
            </>
          )}
          {!loading && !error && kind === "image" && imageUrl && (
            <img className="artifact-preview-image" src={imageUrl} alt={artifact.filename} />
          )}
          {!loading && !error && kind === "binary" && (
            <p className="panel-empty">该文件类型不支持内联预览，请下载查看。</p>
          )}
        </div>
      </div>
    </div>
  );
}
