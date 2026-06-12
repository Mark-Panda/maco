import { downloadArtifact, type ArtifactRecord } from "../api/client";
import { ArtifactPreviewContent } from "./ArtifactPreviewContent";

type Props = {
  sessionId: string;
  artifact: Pick<ArtifactRecord, "id" | "filename" | "mime_type" | "size_bytes">;
  onClose: () => void;
};

export function ArtifactPreviewModal({ sessionId, artifact, onClose }: Props) {
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
                downloadArtifact(sessionId, artifact.id, artifact.filename).catch(() => undefined)
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
          <ArtifactPreviewContent sessionId={sessionId} artifact={artifact} />
        </div>
      </div>
    </div>
  );
}
