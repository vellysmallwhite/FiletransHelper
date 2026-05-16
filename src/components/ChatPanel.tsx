import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ChatMessage, DeviceWithWsState, FileTransfer } from "../api/types";

const MAX_TEXT_BYTES = 10 * 1024 * 1024;

type ChatPanelProps = {
  device: DeviceWithWsState | null;
  messages: ChatMessage[];
  transfers: FileTransfer[];
  loading: boolean;
  error: string | null;
  onSend: (content: string) => Promise<void>;
  onSendFile: (filePath: string) => Promise<void>;
  onRetry: (messageId: string) => Promise<void>;
};

type ThreadItem =
  | { kind: "message"; createdAt: number; id: string; message: ChatMessage }
  | { kind: "transfer"; createdAt: number; id: string; transfer: FileTransfer };

function byteSize(content: string) {
  return new Blob([content]).size;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function formatTime(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString();
}

function messageProgress(message: ChatMessage) {
  if (message.totalChunks <= 0) {
    return 0;
  }

  return Math.min(100, Math.round((message.chunksDone / message.totalChunks) * 100));
}

function transferStatusText(status: FileTransfer["status"]) {
  switch (status) {
    case "sending":
      return "发送中";
    case "sent":
      return "已发送";
    case "receiving":
      return "接收中";
    case "received":
      return "已接收";
    case "failed":
      return "失败";
    default:
      return status;
  }
}

function fileDropLabel(
  hovering: boolean,
  pendingPath: string | null,
  device: DeviceWithWsState | null,
) {
  if (!device) {
    return "选择设备后可拖拽单个文件";
  }

  if (pendingPath) {
    return "文件上传中";
  }

  return hovering ? "松开发送文件" : "拖拽单个文件到这里发送";
}

export function ChatPanel({
  device,
  messages,
  transfers,
  loading,
  error,
  onSend,
  onSendFile,
  onRetry,
}: ChatPanelProps) {
  const [draft, setDraft] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [dropHovering, setDropHovering] = useState(false);
  const [dropError, setDropError] = useState<string | null>(null);
  const [pendingFilePath, setPendingFilePath] = useState<string | null>(null);
  const dropZoneRef = useRef<HTMLDivElement | null>(null);
  const draftBytes = useMemo(() => byteSize(draft), [draft]);
  const canSend = Boolean(device) && draft.trim().length > 0 && draftBytes <= MAX_TEXT_BYTES;
  const threadItems = useMemo(() => {
    const items: ThreadItem[] = [
      ...messages.map((message) => ({
        kind: "message" as const,
        createdAt: message.createdAt,
        id: message.id,
        message,
      })),
      ...transfers.map((transfer) => ({
        kind: "transfer" as const,
        createdAt: transfer.createdAt,
        id: transfer.id,
        transfer,
      })),
    ];

    return items.sort((left, right) => {
      if (left.createdAt !== right.createdAt) {
        return left.createdAt - right.createdAt;
      }

      return left.id.localeCompare(right.id);
    });
  }, [messages, transfers]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSend) {
      return;
    }

    const content = draft;
    setDraft("");
    await onSend(content);
  }

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    async function pointInDropZone(x: number, y: number) {
      const zone = dropZoneRef.current;
      if (!zone) {
        return false;
      }

      const scaleFactor = await getCurrentWindow().scaleFactor();
      const logical = { x: x / scaleFactor, y: y / scaleFactor };
      const rect = zone.getBoundingClientRect();
      return (
        logical.x >= rect.left &&
        logical.x <= rect.right &&
        logical.y >= rect.top &&
        logical.y <= rect.bottom
      );
    }

    getCurrentWebview()
      .onDragDropEvent(async (event) => {
        if (disposed) {
          return;
        }

        if (event.payload.type === "enter") {
          const overZone = await pointInDropZone(
            event.payload.position.x,
            event.payload.position.y,
          );
          setDropHovering(overZone);
          return;
        }

        if (event.payload.type === "over") {
          const overZone = await pointInDropZone(
            event.payload.position.x,
            event.payload.position.y,
          );
          setDropHovering(overZone);
          return;
        }

        if (event.payload.type === "leave") {
          setDropHovering(false);
          return;
        }

        const overZone = await pointInDropZone(
          event.payload.position.x,
          event.payload.position.y,
        );
        setDropHovering(false);
        if (!overZone || !device || pendingFilePath) {
          return;
        }

        if (event.payload.paths.length !== 1) {
          setDropError("Phase 5 仅支持一次拖拽一个文件");
          return;
        }

        const filePath = event.payload.paths[0];
        setDropError(null);
        setPendingFilePath(filePath);
        try {
          await onSendFile(filePath);
        } finally {
          setPendingFilePath(null);
        }
      })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch(() => {
        setDropHovering(false);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [device, onSendFile, pendingFilePath]);

  if (!device) {
    return (
      <section className="chat-panel" aria-label="聊天">
        <div className="empty-state chat-empty">
          <span className="empty-kicker">Chat</span>
          <p>选择一个已添加设备后，可以发送文本和单个文件。</p>
        </div>
      </section>
    );
  }

  return (
    <section className="chat-panel" aria-label={`${device.name} 聊天`}>
      <div className="chat-header">
        <div>
          <p className="eyebrow">Conversation</p>
          <h2>{device.name}</h2>
          <span className="chat-address mono">
            {device.ip}:{device.port}
          </span>
        </div>
        <div className="chat-state-stack">
          <span className={`state-pill ${device.online ? "state-online" : "state-offline"}`}>
            HTTP {device.online ? "online" : "offline"}
          </span>
          <span
            className={`state-pill ${
              device.wsConnected ? "state-online" : "state-offline"
            }`}
          >
            WS {device.wsConnected ? "connected" : device.wsState}
          </span>
        </div>
      </div>

      {dropError || error ? (
        <div className="error-banner">{dropError ?? error}</div>
      ) : null}

      <div
        ref={dropZoneRef}
        className={`file-drop-zone ${dropHovering ? "file-drop-zone-active" : ""}`}
        aria-label="拖拽单个文件发送"
      >
        <span className="file-drop-icon" aria-hidden="true">
          +
        </span>
        <div>
          <strong>{fileDropLabel(dropHovering, pendingFilePath, device)}</strong>
          <span>最大 100 MB，文件夹和多文件会被拒绝</span>
        </div>
      </div>

      <div className="message-thread">
        {loading ? (
          <div className="empty-state thread-empty">
            <span className="empty-kicker">Loading</span>
            <p>正在加载消息历史。</p>
          </div>
        ) : threadItems.length === 0 ? (
          <div className="empty-state thread-empty">
            <span className="empty-kicker">No activity</span>
            <p>发送第一条消息，或拖拽一个普通文件给对方。</p>
          </div>
        ) : (
          threadItems.map((item) => {
            if (item.kind === "transfer") {
              const transfer = item.transfer;
              const isOutbound = transfer.direction === "outbound";

              return (
                <article
                  key={transfer.id}
                  className={`transfer-card ${
                    isOutbound ? "message-out" : "message-in"
                  }`}
                >
                  <div className="transfer-head">
                    <span className="transfer-icon" aria-hidden="true">
                      {isOutbound ? "↑" : "↓"}
                    </span>
                    <div>
                      <strong>{transfer.filename}</strong>
                      <span>
                        {isOutbound ? "发出" : "收到"} · {formatBytes(transfer.size)}
                      </span>
                    </div>
                  </div>
                  <div className="message-footer">
                    <span>{formatTime(transfer.createdAt)}</span>
                    <span>{transferStatusText(transfer.status)}</span>
                  </div>
                  {transfer.localPath ? (
                    <div className="transfer-path mono">{transfer.localPath}</div>
                  ) : null}
                  {transfer.status === "failed" ? (
                    <div className="message-error">
                      <span>{transfer.error ?? "文件传输失败"}</span>
                    </div>
                  ) : null}
                </article>
              );
            }

            const message = item.message;
            const isOutbound = message.direction === "outbound";
            const isLong = message.contentSize > 8 * 1024;
            const isExpanded = expanded[message.id] ?? !isLong;
            const visibleContent = isExpanded
              ? message.content
              : `${message.content.slice(0, 420)}...`;
            const progress = messageProgress(message);

            return (
              <article
                key={message.id}
                className={`message-bubble ${isOutbound ? "message-out" : "message-in"}`}
              >
                <div className="message-copy">{visibleContent}</div>
                {isLong ? (
                  <button
                    type="button"
                    className="link-button"
                    onClick={() =>
                      setExpanded((current) => ({
                        ...current,
                        [message.id]: !isExpanded,
                      }))
                    }
                  >
                    {isExpanded ? "收起长文本" : "展开长文本"}
                  </button>
                ) : null}
                <div className="message-footer">
                  <span>{formatTime(message.createdAt)}</span>
                  <span>{formatBytes(message.contentSize)}</span>
                  <span>{message.status}</span>
                </div>
                {message.status === "sending" || message.status === "receiving" ? (
                  <div className="message-progress" aria-label={`进度 ${progress}%`}>
                    <span style={{ width: `${progress}%` }} />
                  </div>
                ) : null}
                {message.status === "failed" ? (
                  <div className="message-error">
                    <span>{message.error ?? "发送失败"}</span>
                    <button
                      type="button"
                      className="link-button"
                      onClick={() => void onRetry(message.id)}
                    >
                      重试
                    </button>
                  </div>
                ) : null}
              </article>
            );
          })
        )}
      </div>

      <form className="message-input" onSubmit={handleSubmit}>
        <textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="输入消息，支持最长 10MB 文本"
          rows={5}
        />
        <div className="message-input-footer">
          <span className={draftBytes > MAX_TEXT_BYTES ? "limit-error" : ""}>
            {formatBytes(draftBytes)} / 10 MB
          </span>
          <button type="submit" className="primary-button" disabled={!canSend}>
            发送
          </button>
        </div>
      </form>
    </section>
  );
}
