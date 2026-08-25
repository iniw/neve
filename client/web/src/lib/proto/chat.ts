import { ChatServiceDefinition } from "./generated/neve/server/v1/chat.ts";
import { createClient } from "./rpc.ts";

export const chatService = createClient(ChatServiceDefinition);
