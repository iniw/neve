import { AuthServiceDefinition } from "./generated/neve/server/v1/auth.ts";
import { createClient } from "./rpc.ts";

export const authService = createClient(AuthServiceDefinition);
