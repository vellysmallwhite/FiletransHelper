import type {
  Device,
  DeviceWithWsState,
  WsConnectionChangedEvent,
  WsConnectionInfo,
} from "../api/types";

export type DeviceState = {
  devices: DeviceWithWsState[];
  selectedDeviceId: string | null;
  loading: boolean;
  error: string | null;
};

export const initialDeviceState: DeviceState = {
  devices: [],
  selectedDeviceId: null,
  loading: false,
  error: null,
};

export function selectDefaultDevice(
  devices: Array<{ id: string }>,
  currentId: string | null,
): string | null {
  if (currentId && devices.some((device) => device.id === currentId)) {
    return currentId;
  }

  return devices[0]?.id ?? null;
}

export function mergeDeviceRuntimeState(
  devices: Device[],
  currentDevices: DeviceWithWsState[],
): DeviceWithWsState[] {
  const currentById = new Map(currentDevices.map((device) => [device.id, device]));

  return devices.map((device) => {
    const current = currentById.get(device.id);
    return {
      ...device,
      wsConnected: current?.wsConnected ?? false,
      wsState: current?.wsState ?? "disconnected",
      lastWsEventAt: current?.lastWsEventAt ?? null,
    };
  });
}

export function applyWsConnections(
  devices: DeviceWithWsState[],
  connections: WsConnectionInfo[],
): DeviceWithWsState[] {
  const connectionById = new Map(
    connections.map((connection) => [connection.peerDeviceId, connection]),
  );

  return devices.map((device) => {
    const connection = connectionById.get(device.id);
    if (!connection) {
      return {
        ...device,
        wsConnected: false,
        wsState: device.wsState === "connected" ? "disconnected" : device.wsState,
      };
    }

    return {
      ...device,
      wsConnected: connection.connected,
      wsState: connection.state,
      lastWsEventAt: connection.lastEventAt ?? device.lastWsEventAt ?? null,
    };
  });
}

export function applyWsConnectionChange(
  devices: DeviceWithWsState[],
  event: WsConnectionChangedEvent,
): DeviceWithWsState[] {
  return devices.map((device) => {
    if (device.id !== event.peerDeviceId) {
      return device;
    }

    return {
      ...device,
      wsConnected: event.connected,
      wsState: event.state,
      lastWsEventAt: event.lastEventAt ?? Date.now(),
    };
  });
}
