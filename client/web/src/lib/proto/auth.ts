import { AuthServiceDefinition } from "./generated/neve/server/v1/auth.ts";
import { createUnauthenticatedClient } from "./rpc.ts";

const AUTH_TOKEN_STORAGE_KEY = "neve.authToken";

export const authService = createUnauthenticatedClient(AuthServiceDefinition);

/** Saves the token used to authenticate later RPC calls. */
export function storeAuthToken(authToken: string): void {
  sessionStorage.setItem(AUTH_TOKEN_STORAGE_KEY, authToken);
}

/** Reads the token used to authenticate RPC calls. */
export function readAuthToken(): string | null {
  return sessionStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}
