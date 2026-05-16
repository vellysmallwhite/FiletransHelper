import type { ChatMessage } from "../api/types";

export type MessageState = {
  messagesByPeer: Record<string, ChatMessage[]>;
  loadingPeerId: string | null;
  error: string | null;
};

export const initialMessageState: MessageState = {
  messagesByPeer: {},
  loadingPeerId: null,
  error: null,
};

export function upsertMessage(
  messages: ChatMessage[],
  nextMessage: ChatMessage,
): ChatMessage[] {
  const existingIndex = messages.findIndex((message) => message.id === nextMessage.id);
  if (existingIndex === -1) {
    return [...messages, nextMessage].sort(compareMessages);
  }

  const nextMessages = [...messages];
  nextMessages[existingIndex] = nextMessage;
  return nextMessages.sort(compareMessages);
}

function compareMessages(left: ChatMessage, right: ChatMessage) {
  if (left.createdAt !== right.createdAt) {
    return left.createdAt - right.createdAt;
  }

  return left.id.localeCompare(right.id);
}
