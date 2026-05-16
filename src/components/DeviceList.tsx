import { FormEvent, useMemo, useState } from "react";
import type { DeviceWithWsState } from "../api/types";

type DeviceListProps = {
  devices: DeviceWithWsState[];
  selectedDeviceId: string | null;
  loading: boolean;
  error: string | null;
  onAdd: (address: string) => Promise<void>;
  onRefresh: () => Promise<void>;
  onSelect: (deviceId: string) => void;
};

function formatTimestamp(timestamp?: number | null) {
  if (!timestamp) {
    return "尚未在线";
  }

  return new Date(timestamp).toLocaleString();
}

function formatRelative(timestamp?: number | null) {
  if (!timestamp) {
    return "未检查";
  }

  const seconds = Math.max(1, Math.floor((Date.now() - timestamp) / 1000));
  if (seconds < 60) {
    return `${seconds} 秒前`;
  }

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes} 分钟前`;
  }

  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours} 小时前`;
  }

  return `${Math.floor(hours / 24)} 天前`;
}

export function DeviceList({
  devices,
  selectedDeviceId,
  loading,
  error,
  onAdd,
  onRefresh,
  onSelect,
}: DeviceListProps) {
  const [address, setAddress] = useState("127.0.0.1:8765");
  const selectedDevice = useMemo(
    () => devices.find((device) => device.id === selectedDeviceId) ?? null,
    [devices, selectedDeviceId],
  );

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onAdd(address);
  }

  return (
    <section className="device-panel" aria-label="设备列表">
      <div className="panel-header">
        <div>
          <p className="eyebrow">ZeroTier Peers</p>
          <h2>设备列表</h2>
        </div>
        <button
          type="button"
          className="secondary-button"
          onClick={onRefresh}
          disabled={loading}
        >
          {loading ? "刷新中" : "刷新状态"}
        </button>
      </div>

      <form className="add-device-form" onSubmit={handleSubmit}>
        <label htmlFor="device-address">手动添加</label>
        <div className="address-control">
          <input
            id="device-address"
            value={address}
            onChange={(event) => setAddress(event.target.value)}
            placeholder="PEER_ZEROTIER_IP:8765"
            spellCheck={false}
          />
          <button type="submit" className="primary-button" disabled={loading}>
            添加
          </button>
        </div>
      </form>

      {error ? <div className="error-banner">{error}</div> : null}

      <div className="device-list" role="list">
        {devices.length === 0 ? (
          <div className="empty-state">
            <span className="empty-kicker">No peers</span>
            <p>输入 ZeroTier 地址后，应用会调用对端 /api/ping 并保存设备。</p>
          </div>
        ) : (
          devices.map((device) => (
            <button
              type="button"
              key={device.id}
              className={`device-row ${
                selectedDeviceId === device.id ? "device-row-active" : ""
              }`}
              onClick={() => onSelect(device.id)}
            >
              <span className="device-status-stack">
                <span
                  className={`device-status ${device.online ? "is-online" : "is-offline"}`}
                  aria-label={device.online ? "HTTP online" : "HTTP offline"}
                />
                <span
                  className={`device-status ws-dot ${
                    device.wsConnected ? "is-ws-online" : "is-ws-offline"
                  }`}
                  aria-label={
                    device.wsConnected ? "WebSocket connected" : "WebSocket disconnected"
                  }
                />
              </span>
              <span className="device-main">
                <span className="device-title">{device.name}</span>
                <span className="device-address mono">
                  {device.ip}:{device.port}
                </span>
              </span>
              <span className="device-meta">
                <span>HTTP {device.online ? "online" : "offline"}</span>
                <span>WS {device.wsConnected ? "connected" : device.wsState}</span>
                <span>{formatRelative(device.lastCheckedAt ?? device.lastSeenAt)}</span>
              </span>
            </button>
          ))
        )}
      </div>

      <div className="device-detail" aria-label="设备详情">
        {selectedDevice ? (
          <>
            <div className="device-detail-head">
              <div>
                <p className="eyebrow">Selected Peer</p>
                <h2>{selectedDevice.name}</h2>
              </div>
              <span
                className={`state-pill ${
                  selectedDevice.wsConnected ? "state-online" : "state-offline"
                }`}
              >
                WS {selectedDevice.wsConnected ? "connected" : selectedDevice.wsState}
              </span>
            </div>
            <dl className="info-grid compact-info">
              <div>
                <dt>Device ID</dt>
                <dd className="mono">{selectedDevice.id}</dd>
              </div>
              <div>
                <dt>地址</dt>
                <dd className="mono">
                  {selectedDevice.ip}:{selectedDevice.port}
                </dd>
              </div>
              <div>
                <dt>平台</dt>
                <dd>{selectedDevice.platform ?? "-"}</dd>
              </div>
              <div>
                <dt>版本</dt>
                <dd>{selectedDevice.version ?? "-"}</dd>
              </div>
              <div>
                <dt>HTTP 最近在线</dt>
                <dd>{formatTimestamp(selectedDevice.lastSeenAt)}</dd>
              </div>
              <div>
                <dt>最近检查</dt>
                <dd>{formatTimestamp(selectedDevice.lastCheckedAt)}</dd>
              </div>
              <div>
                <dt>控制通道</dt>
                <dd>{selectedDevice.wsConnected ? "connected" : selectedDevice.wsState}</dd>
              </div>
              <div>
                <dt>最近 WS 事件</dt>
                <dd>{formatTimestamp(selectedDevice.lastWsEventAt)}</dd>
              </div>
            </dl>
            {selectedDevice.lastError ? (
              <div className="error-banner">{selectedDevice.lastError}</div>
            ) : null}
            <div className="feature-strip">
              {(selectedDevice.features.length ? selectedDevice.features : ["ping"]).map(
                (feature) => (
                  <span key={feature}>{feature}</span>
                ),
              )}
            </div>
          </>
        ) : (
          <div className="empty-state detail-empty">
            <span className="empty-kicker">Ready</span>
            <p>添加设备后可在这里查看在线状态、最近在线时间和基础能力。</p>
          </div>
        )}
      </div>
    </section>
  );
}
