import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AgentStatusEvent,
  ChatMessage,
  Device,
  DeviceWsEvent,
  FileTransfer,
  LocalInfo,
  MessageReceivedEvent,
  MessageStatusChangedEvent,
  TransferCreatedEvent,
  TransferStatusChangedEvent,
  WsConnectionChangedEvent,
  WsConnectionInfo,
} from "./types";

export function getLocalInfo(): Promise<LocalInfo> {
  return invoke<LocalInfo>("get_local_info");
}

export function helloFromRust(name: string): Promise<string> {
  return invoke<string>("hello_from_rust", { name });
}

export function listDevices(): Promise<Device[]> {
  return invoke<Device[]>("list_devices");
}

export function addDevice(address: string): Promise<Device> {
  return invoke<Device>("add_device", { address });
}

export function refreshDeviceStatus(): Promise<Device[]> {
  return invoke<Device[]>("refresh_device_status");
}

export function getMessages(peerDeviceId: string): Promise<ChatMessage[]> {
  return invoke<ChatMessage[]>("get_messages", { peerDeviceId });
}

export function getTransfers(peerDeviceId?: string): Promise<FileTransfer[]> {
  return invoke<FileTransfer[]>("get_transfers", { peerDeviceId });
}

export function sendText(
  peerDeviceId: string,
  content: string,
): Promise<ChatMessage> {
  return invoke<ChatMessage>("send_text", { peerDeviceId, content });
}

export function sendFile(
  peerDeviceId: string,
  filePath: string,
): Promise<FileTransfer> {
  return invoke<FileTransfer>("send_file", { peerDeviceId, filePath });
}

export function retryMessage(messageId: string): Promise<ChatMessage> {
  return invoke<ChatMessage>("retry_message", { messageId });
}

export function connectDeviceWs(peerDeviceId: string): Promise<void> {
  return invoke<void>("connect_device_ws", { peerDeviceId });
}

export function connectAllDeviceWs(): Promise<void> {
  return invoke<void>("connect_all_device_ws");
}

export function listWsConnections(): Promise<WsConnectionInfo[]> {
  return invoke<WsConnectionInfo[]>("list_ws_connections");
}

export function listenAgentStatus(
  handler: (payload: AgentStatusEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentStatusEvent>("agent_status", (event) => handler(event.payload));
}

export function listenMessageReceived(
  handler: (payload: MessageReceivedEvent) => void,
): Promise<UnlistenFn> {
  return listen<MessageReceivedEvent>("message_received", (event) =>
    handler(event.payload),
  );
}

export function listenMessageStatusChanged(
  handler: (payload: MessageStatusChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<MessageStatusChangedEvent>("message_status_changed", (event) =>
    handler(event.payload),
  );
}

export function listenTransferCreated(
  handler: (payload: TransferCreatedEvent) => void,
): Promise<UnlistenFn> {
  return listen<TransferCreatedEvent>("transfer_created", (event) =>
    handler(event.payload),
  );
}

export function listenTransferStatusChanged(
  handler: (payload: TransferStatusChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<TransferStatusChangedEvent>(
    "transfer_status_changed",
    (event) => handler(event.payload),
  );
}

export function listenDeviceOnline(
  handler: (payload: DeviceWsEvent) => void,
): Promise<UnlistenFn> {
  return listen<DeviceWsEvent>("device_online", (event) => handler(event.payload));
}

export function listenDeviceOffline(
  handler: (payload: DeviceWsEvent) => void,
): Promise<UnlistenFn> {
  return listen<DeviceWsEvent>("device_offline", (event) => handler(event.payload));
}

export function listenWsConnectionChanged(
  handler: (payload: WsConnectionChangedEvent) => void,
): Promise<UnlistenFn> {
  return listen<WsConnectionChangedEvent>("ws_connection_changed", (event) =>
    handler(event.payload),
  );
}
