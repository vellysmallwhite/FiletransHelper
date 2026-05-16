export type ServerStatus = {
  state: "stopped" | "starting" | "running" | "failed";
  bindAddr: string;
  error?: string | null;
};

export type LocalInfo = {
  deviceId: string;
  deviceName: string;
  version: string;
  platform: string;
  protocolVersion: number;
  listenHost: string;
  listenPort: number;
  downloadDir: string;
  configPath: string;
  databasePath: string;
  features: string[];
  serverStatus: ServerStatus;
};

export type AgentStatusEvent = {
  timestamp: number;
  localInfo: LocalInfo;
};

export type Device = {
  id: string;
  name: string;
  ip: string;
  port: number;
  publicKey?: string | null;
  trusted: boolean;
  platform?: string | null;
  version?: string | null;
  protocolVersion?: number | null;
  features: string[];
  online: boolean;
  createdAt: number;
  lastSeenAt?: number | null;
  lastCheckedAt?: number | null;
  lastError?: string | null;
};

export type WsConnectionInfo = {
  peerDeviceId: string;
  peerDeviceName?: string | null;
  connected: boolean;
  state: string;
  lastEventAt?: number | null;
};

export type WsEvent = {
  eventId: string;
  eventType:
    | "hello"
    | "heartbeat"
    | "deviceOnline"
    | "deviceOffline"
    | "messageReceived"
    | "transferProgress"
    | "transferStatusChanged"
    | string;
  fromDeviceId: string;
  fromDeviceName: string;
  createdAt: number;
  payload: unknown;
};

export type DeviceWsEvent = {
  peerDeviceId: string;
  peerDeviceName?: string | null;
  event: WsEvent;
};

export type WsConnectionChangedEvent = {
  peerDeviceId: string;
  peerDeviceName?: string | null;
  connected: boolean;
  state: string;
  lastEventAt?: number | null;
  error?: string | null;
};

export type DeviceWithWsState = Device & {
  wsConnected: boolean;
  wsState: string;
  lastWsEventAt?: number | null;
};

export type ChatMessage = {
  id: string;
  peerDeviceId: string;
  peerDeviceName?: string | null;
  direction: "inbound" | "outbound";
  content: string;
  status: "sending" | "sent" | "failed" | "receiving" | "received";
  contentSize: number;
  chunkSize: number;
  totalChunks: number;
  chunksDone: number;
  createdAt: number;
  updatedAt: number;
  error?: string | null;
};

export type FileTransfer = {
  id: string;
  peerDeviceId: string;
  peerDeviceName?: string | null;
  direction: "inbound" | "outbound";
  filename: string;
  size: number;
  status: "sending" | "sent" | "failed" | "receiving" | "received";
  localPath?: string | null;
  createdAt: number;
  updatedAt: number;
  error?: string | null;
};

export type MessageReceivedEvent = {
  message: ChatMessage;
};

export type MessageStatusChangedEvent = {
  message: ChatMessage;
};

export type TransferCreatedEvent = {
  transfer: FileTransfer;
};

export type TransferStatusChangedEvent = {
  transfer: FileTransfer;
};
