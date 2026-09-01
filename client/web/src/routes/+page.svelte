<script lang="ts">
import { goto } from "$app/navigation";
import { authService } from "$lib/proto/auth";
import { storeAuthToken } from "$lib/proto/auth-token";

let username = $state("");
let password = $state("");
let authentication = $state<
  | { state: "idle" }
  | { state: "submitting" }
  | { state: "error"; message: string }
>({ state: "idle" });

async function submit(event: SubmitEvent) {
  event.preventDefault();
  authentication = { state: "submitting" };

  try {
    const { authToken } = await authService.authenticateOrRegister({
      username,
      password,
    });

    storeAuthToken(authToken);

    await goto("/chats/new");
  } catch (error) {
    authentication = {
      state: "error",
      message: error instanceof Error ? error.message : "Authentication failed",
    };
  }
}
</script>

<svelte:head>
  <title>Sign in | Neve</title>
</svelte:head>

<main class="centered-page">
  <form class="stacked-form" onsubmit={submit}>
		<h1>Sign in</h1>

		<label for="username">Username</label>
		<input
			id="username"
			name="username"
			autocomplete="username"
			bind:value={username}
			disabled={authentication.state === "submitting"}
			required
		/>

		<label for="password">Password</label>
		<input
			id="password"
			name="password"
			type="password"
			autocomplete="current-password"
			bind:value={password}
			disabled={authentication.state === "submitting"}
			required
		/>

		<button type="submit" disabled={authentication.state === "submitting"}>
			{authentication.state === "submitting" ? "Signing in…" : "Sign in"}
		</button>

		{#if authentication.state === "error"}
			<p role="alert">{authentication.message}</p>
		{/if}
  </form>
</main>
