import { useCallback, useEffect, useState } from "react";
import {
  addDevice,
  connectAllDeviceWs,
  getLocalInfo,
  getMessages,
  getTransfers,
  listenDeviceOffline,
  listenDeviceOnline,
  listenAgentStatus,
  listenMessageReceived,
  listenMessageStatusChanged,
  listenTransferCreated,
  listenTransferStatusChanged,
  listenWsConnectionChanged,
  listDevices,
  listWsConnections,
  retryMessage,
  refreshDeviceStatus,
  sendFile,
  sendText,
} from "./api/tauri";
import type { AgentStatusEvent, LocalInfo } from "./api/types";
import { ChatPanel } from "./components/ChatPanel";
import { DeviceList } from "./components/DeviceList";
import { SettingsPanel } from "./components/SettingsPanel";
import {
  applyWsConnectionChange,
  applyWsConnections,
  initialDeviceState,
  mergeDeviceRuntimeState,
  selectDefaultDevice,
} from "./store/deviceStore";
import { initialMessageState, upsertMessage } from "./store/messageStore";
import { initialSettingsState } from "./store/settingsStore";
import { initialTransferState, upsertTransfer } from "./store/transferStore";

export default function App() {
  const [localInfo, setLocalInfo] = useState<LocalInfo | null>(
    initialSettingsState.localInfo,
  );
  const [lastEvent, setLastEvent] = useState<AgentStatusEvent | null>(
    initialSettingsState.lastEvent,
  );
  const [error, setError] = useState<string | null>(initialSettingsState.error);
  const [devices, setDevices] = useState(initialDeviceState.devices);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(
    initialDeviceState.selectedDeviceId,
  );
  const [devicesLoading, setDevicesLoading] = useState(initialDeviceState.loading);
  const [deviceError, setDeviceError] = useState(initialDeviceState.error);
  const [messagesByPeer, setMessagesByPeer] = useState(
    initialMessageState.messagesByPeer,
  );
  const [messageLoadingPeerId, setMessageLoadingPeerId] = useState<string | null>(
    initialMessageState.loadingPeerId,
  );
  const [messageError, setMessageError] = useState(initialMessageState.error);
  const [transfersByPeer, setTransfersByPeer] = useState(
    initialTransferState.transfersByPeer,
  );
  const [transferLoadingPeerId, setTransferLoadingPeerId] = useState<string | null>(
    initialTransferState.loadingPeerId,
  );
  const [transferError, setTransferError] = useState(initialTransferState.error);

  const selectedDevice =
    devices.find((device) => device.id === selectedDeviceId) ?? null;
  const selectedMessages = selectedDeviceId
    ? messagesByPeer[selectedDeviceId] ?? []
    : [];
  const selectedTransfers = selectedDeviceId
    ? transfersByPeer[selectedDeviceId] ?? []
    : [];
  const chatError = messageError ?? transferError;
  const chatLoading =
    messageLoadingPeerId === selectedDeviceId ||
    transferLoadingPeerId === selectedDeviceId;

  const refreshLocalInfo = useCallback(async () => {
    try {
      const info = await getLocalInfo();
      setLocalInfo(info);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const loadDevices = useCallback(async () => {
    try {
      setDevicesLoading(true);
      const nextDevices = await listDevices();
      const wsConnections = await listWsConnections();
      setDevices((current) =>
        applyWsConnections(
          mergeDeviceRuntimeState(nextDevices, current),
          wsConnections,
        ),
      );
      setSelectedDeviceId((currentId) => selectDefaultDevice(nextDevices, currentId));
      setDeviceError(null);
    } catch (err) {
      setDeviceError(err instanceof Error ? err.message : String(err));
    } finally {
      setDevicesLoading(false);
    }
  }, []);

  const handleRefreshDevices = useCallback(async () => {
    try {
      setDevicesLoading(true);
      const nextDevices = await refreshDeviceStatus();
      const wsConnections = await listWsConnections();
      setDevices((current) =>
        applyWsConnections(
          mergeDeviceRuntimeState(nextDevices, current),
          wsConnections,
        ),
      );
      setSelectedDeviceId((currentId) => selectDefaultDevice(nextDevices, currentId));
      setDeviceError(null);
    } catch (err) {
      setDeviceError(err instanceof Error ? err.message : String(err));
    } finally {
      setDevicesLoading(false);
    }
  }, []);

  const handleAddDevice = useCallback(async (address: string) => {
    try {
      setDevicesLoading(true);
      const added = await addDevice(address);
      const nextDevices = await listDevices();
      const wsConnections = await listWsConnections();
      setDevices((current) =>
        applyWsConnections(
          mergeDeviceRuntimeState(nextDevices, current),
          wsConnections,
        ),
      );
      setSelectedDeviceId(added.id);
      setDeviceError(null);
    } catch (err) {
      setDeviceError(err instanceof Error ? err.message : String(err));
    } finally {
      setDevicesLoading(false);
    }
  }, []);

  const loadMessages = useCallback(async (peerDeviceId: string) => {
    try {
      setMessageLoadingPeerId(peerDeviceId);
      const nextMessages = await getMessages(peerDeviceId);
      setMessagesByPeer((current) => ({
        ...current,
        [peerDeviceId]: nextMessages,
      }));
      setMessageError(null);
    } catch (err) {
      setMessageError(err instanceof Error ? err.message : String(err));
    } finally {
      setMessageLoadingPeerId((current) =>
        current === peerDeviceId ? null : current,
      );
    }
  }, []);

  const loadTransfers = useCallback(async (peerDeviceId: string) => {
    try {
      setTransferLoadingPeerId(peerDeviceId);
      const nextTransfers = await getTransfers(peerDeviceId);
      setTransfersByPeer((current) => ({
        ...current,
        [peerDeviceId]: nextTransfers,
      }));
      setTransferError(null);
    } catch (err) {
      setTransferError(err instanceof Error ? err.message : String(err));
    } finally {
      setTransferLoadingPeerId((current) =>
        current === peerDeviceId ? null : current,
      );
    }
  }, []);

  const handleSendText = useCallback(
    async (content: string) => {
      if (!selectedDeviceId) {
        setMessageError("请先选择设备");
        return;
      }

      try {
        const message = await sendText(selectedDeviceId, content);
        setMessagesByPeer((current) => ({
          ...current,
          [message.peerDeviceId]: upsertMessage(
            current[message.peerDeviceId] ?? [],
            message,
          ),
        }));
        setMessageError(null);
      } catch (err) {
        setMessageError(err instanceof Error ? err.message : String(err));
      }
    },
    [selectedDeviceId],
  );

  const handleSendFile = useCallback(
    async (filePath: string) => {
      if (!selectedDeviceId) {
        setTransferError("请先选择设备");
        return;
      }

      try {
        const transfer = await sendFile(selectedDeviceId, filePath);
        setTransfersByPeer((current) => ({
          ...current,
          [transfer.peerDeviceId]: upsertTransfer(
            current[transfer.peerDeviceId] ?? [],
            transfer,
          ),
        }));
        setTransferError(null);
      } catch (err) {
        setTransferError(err instanceof Error ? err.message : String(err));
      }
    },
    [selectedDeviceId],
  );

  const handleRetryMessage = useCallback(async (messageId: string) => {
    try {
      const message = await retryMessage(messageId);
      setMessagesByPeer((current) => ({
        ...current,
        [message.peerDeviceId]: upsertMessage(
          current[message.peerDeviceId] ?? [],
          message,
        ),
      }));
      setMessageError(null);
    } catch (err) {
      setMessageError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void refreshLocalInfo();
    void loadDevices();
    void connectAllDeviceWs().catch((err) => {
      setDeviceError(err instanceof Error ? err.message : String(err));
    });

    let disposed = false;
    let unlisten: (() => void) | undefined;

    listenAgentStatus((payload) => {
      setLastEvent(payload);
      setLocalInfo(payload.localInfo);
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : String(err));
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadDevices, refreshLocalInfo]);

  useEffect(() => {
    let disposed = false;

    listWsConnections()
      .then((connections) => {
        if (disposed) {
          return;
        }
        setDevices((current) => applyWsConnections(current, connections));
      })
      .catch((err) => setDeviceError(err instanceof Error ? err.message : String(err)));

    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    if (!selectedDeviceId) {
      return;
    }

    void loadMessages(selectedDeviceId);
    void loadTransfers(selectedDeviceId);
  }, [loadMessages, loadTransfers, selectedDeviceId]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    listenMessageReceived(({ message }) => {
      setMessagesByPeer((current) => ({
        ...current,
        [message.peerDeviceId]: upsertMessage(
          current[message.peerDeviceId] ?? [],
          message,
        ),
      }));
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setMessageError(err instanceof Error ? err.message : String(err)));

    listenMessageStatusChanged(({ message }) => {
      setMessagesByPeer((current) => ({
        ...current,
        [message.peerDeviceId]: upsertMessage(
          current[message.peerDeviceId] ?? [],
          message,
        ),
      }));
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setMessageError(err instanceof Error ? err.message : String(err)));

    listenTransferCreated(({ transfer }) => {
      setTransfersByPeer((current) => ({
        ...current,
        [transfer.peerDeviceId]: upsertTransfer(
          current[transfer.peerDeviceId] ?? [],
          transfer,
        ),
      }));
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setTransferError(err instanceof Error ? err.message : String(err)));

    listenTransferStatusChanged(({ transfer }) => {
      setTransfersByPeer((current) => ({
        ...current,
        [transfer.peerDeviceId]: upsertTransfer(
          current[transfer.peerDeviceId] ?? [],
          transfer,
        ),
      }));
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setTransferError(err instanceof Error ? err.message : String(err)));

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    listenWsConnectionChanged((payload) => {
      setDevices((current) => applyWsConnectionChange(current, payload));
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setDeviceError(err instanceof Error ? err.message : String(err)));

    listenDeviceOnline(({ peerDeviceId, event }) => {
      setDevices((current) =>
        applyWsConnectionChange(current, {
          peerDeviceId,
          connected: true,
          state: "connected",
          lastEventAt: event.createdAt,
        }),
      );
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setDeviceError(err instanceof Error ? err.message : String(err)));

    listenDeviceOffline(({ peerDeviceId, event }) => {
      setDevices((current) =>
        applyWsConnectionChange(current, {
          peerDeviceId,
          connected: false,
          state: "disconnected",
          lastEventAt: event.createdAt,
        }),
      );
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisteners.push(fn);
      })
      .catch((err) => setDeviceError(err instanceof Error ? err.message : String(err)));

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      void handleRefreshDevices();
    }, 30_000);

    return () => window.clearInterval(intervalId);
  }, [handleRefreshDevices]);

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div>
          <p className="eyebrow">Private AirDrop</p>
          <h1>ZeroDrop</h1>
        </div>
        <nav className="nav-list" aria-label="主导航">
          <button type="button" className="nav-item">
            本机
          </button>
          <button type="button" className="nav-item nav-item-active">
            聊天
          </button>
          <button type="button" className="nav-item" disabled>
            传输
          </button>
          <button type="button" className="nav-item" disabled>
            设置
          </button>
        </nav>
        <div className="sidebar-note">
          Phase 5：单文件走 HTTP 直接上传，WebSocket 控制通道继续负责在线状态和心跳。
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Direct Upload</p>
            <h2>文本聊天与单文件传输</h2>
          </div>
          <button
            type="button"
            className="primary-button"
            onClick={() => void handleRefreshDevices()}
            disabled={devicesLoading}
          >
            刷新设备
          </button>
        </header>

        <div className="content-grid">
          <DeviceList
            devices={devices}
            selectedDeviceId={selectedDeviceId}
            loading={devicesLoading}
            error={deviceError}
            onAdd={handleAddDevice}
            onRefresh={handleRefreshDevices}
            onSelect={setSelectedDeviceId}
          />

          <div className="right-stack">
            <ChatPanel
              device={selectedDevice}
              messages={selectedMessages}
              transfers={selectedTransfers}
              loading={chatLoading}
              error={chatError}
              onSend={handleSendText}
              onSendFile={handleSendFile}
              onRetry={handleRetryMessage}
            />

            <SettingsPanel
              localInfo={localInfo}
              lastEvent={lastEvent}
              error={error}
              onRefresh={refreshLocalInfo}
            />
          </div>
        </div>
      </section>
    </main>
  );
}
