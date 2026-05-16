import type { AgentStatusEvent, LocalInfo } from "../api/types";

type SettingsPanelProps = {
  localInfo: LocalInfo | null;
  lastEvent: AgentStatusEvent | null;
  error: string | null;
  onRefresh: () => void;
};

function formatTime(timestamp?: number) {
  if (!timestamp) {
    return "尚未收到";
  }

  return new Date(timestamp).toLocaleTimeString();
}

export function SettingsPanel({
  localInfo,
  lastEvent,
  error,
  onRefresh,
}: SettingsPanelProps) {
  const serverState = localInfo?.serverStatus.state ?? "starting";

  return (
    <section className="settings-panel" aria-label="本机节点设置">
      <div className="panel-header">
        <div>
          <p className="eyebrow">Local Agent</p>
          <h2>本机节点</h2>
        </div>
        <button type="button" className="secondary-button" onClick={onRefresh}>
          刷新
        </button>
      </div>

      {error ? <div className="error-banner">{error}</div> : null}

      <div className="status-row">
        <span className={`status-dot status-${serverState}`} aria-hidden="true" />
        <span className="status-copy">
          HTTP Agent {serverState === "running" ? "运行中" : serverState}
        </span>
      </div>

      <dl className="info-grid">
        <div>
          <dt>设备名</dt>
          <dd>{localInfo?.deviceName ?? "加载中"}</dd>
        </div>
        <div>
          <dt>Device ID</dt>
          <dd className="mono">{localInfo?.deviceId ?? "加载中"}</dd>
        </div>
        <div>
          <dt>监听地址</dt>
          <dd className="mono">
            {localInfo ? `${localInfo.listenHost}:${localInfo.listenPort}` : "加载中"}
          </dd>
        </div>
        <div>
          <dt>平台</dt>
          <dd>{localInfo?.platform ?? "加载中"}</dd>
        </div>
        <div>
          <dt>下载目录</dt>
          <dd className="mono">{localInfo?.downloadDir ?? "加载中"}</dd>
        </div>
        <div>
          <dt>配置文件</dt>
          <dd className="mono">{localInfo?.configPath ?? "加载中"}</dd>
        </div>
        <div>
          <dt>设备数据库</dt>
          <dd className="mono">{localInfo?.databasePath ?? "加载中"}</dd>
        </div>
        <div>
          <dt>协议版本</dt>
          <dd>{localInfo?.protocolVersion ?? "-"}</dd>
        </div>
        <div>
          <dt>最近事件</dt>
          <dd>{formatTime(lastEvent?.timestamp)}</dd>
        </div>
      </dl>

      {localInfo?.serverStatus.error ? (
        <div className="error-banner">{localInfo.serverStatus.error}</div>
      ) : null}

      <div className="feature-strip">
        {(localInfo?.features ?? ["ping", "http_transport"]).map((feature) => (
          <span key={feature}>{feature}</span>
        ))}
      </div>
    </section>
  );
}
