const AUTH_TOKEN_STORAGE_KEY = "neve.authToken";

/** Saves the token used to authenticate later RPC calls. */
export function storeAuthToken(authToken: string): void {
  sessionStorage.setItem(AUTH_TOKEN_STORAGE_KEY, authToken);
}

/** Reads the token used to authenticate RPC calls. */
export function readAuthToken(): string | null {
  return sessionStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}
