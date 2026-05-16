import type { FileTransfer } from "../api/types";

export type TransferState = {
  transfersByPeer: Record<string, FileTransfer[]>;
  loadingPeerId: string | null;
  error: string | null;
};

export const initialTransferState: TransferState = {
  transfersByPeer: {},
  loadingPeerId: null,
  error: null,
};

export function upsertTransfer(
  transfers: FileTransfer[],
  nextTransfer: FileTransfer,
): FileTransfer[] {
  const existingIndex = transfers.findIndex(
    (transfer) => transfer.id === nextTransfer.id,
  );
  if (existingIndex === -1) {
    return [...transfers, nextTransfer].sort(compareTransfers);
  }

  const nextTransfers = [...transfers];
  nextTransfers[existingIndex] = nextTransfer;
  return nextTransfers.sort(compareTransfers);
}

function compareTransfers(left: FileTransfer, right: FileTransfer) {
  if (left.createdAt !== right.createdAt) {
    return left.createdAt - right.createdAt;
  }

  return left.id.localeCompare(right.id);
}
