import { AuthServiceDefinition } from "./generated/neve/server/v1/auth.ts";
import { createUnauthenticatedClient } from "./rpc.ts";

export const authService = createUnauthenticatedClient(AuthServiceDefinition);
