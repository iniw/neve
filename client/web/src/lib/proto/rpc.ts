import { SERVER_URL } from "$app/env/public";
import {
  type Client,
  type CompatServiceDefinition,
  createChannel,
  createClientFactory,
  Metadata,
} from "nice-grpc-web";
import { readAuthToken } from "./auth-token.ts";

const channel = createChannel(SERVER_URL);

const authenticatedClientFactory = createClientFactory().use(
  (call, options) => {
    const metadata = Metadata(options.metadata);
    const authToken = readAuthToken();
    if (authToken !== null) {
      metadata.set("neve-auth-token", authToken);
    }

    return call.next(call.request, { ...options, metadata });
  },
);

/** Creates a service client that sends the saved authentication token with each call. */
export function createClient<Service extends CompatServiceDefinition>(
  definition: Service,
): Client<Service> {
  return authenticatedClientFactory.create(definition, channel);
}
