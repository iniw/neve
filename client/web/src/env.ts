import { defineEnvVars } from "@sveltejs/kit/env";

export const variables = defineEnvVars({
  SERVER_URL: {
    public: true,
    static: true,
    description: "The URL of the Neve server.",
  },
});
