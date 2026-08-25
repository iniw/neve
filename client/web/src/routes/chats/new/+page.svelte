<script lang="ts">
import { chatService } from "$lib/proto/chat";

let participantId = $state("");
let name = $state("");
let creation = $state<
  | { state: "idle" }
  | { state: "submitting" }
  | { state: "created"; chatId: number }
  | { state: "error"; message: string }
>({ state: "idle" });

async function submit(event: SubmitEvent) {
  event.preventDefault();
  creation = { state: "submitting" };

  try {
    const { chatId } = await chatService.createChat({
      participants: [Number(participantId)],
      name: name === "" ? undefined : name,
    });
    creation = { state: "created", chatId };
  } catch (error) {
    creation = {
      state: "error",
      message: error instanceof Error ? error.message : "Chat creation failed",
    };
  }
}
</script>

<svelte:head>
  <title>Create chat | Neve</title>
</svelte:head>

<main class="centered-page">
  <form class="stacked-form" onsubmit={submit}>
    <h1>Create chat</h1>

    <label for="participant-id">Participant ID</label>
    <input
      id="participant-id"
      name="participant-id"
      type="number"
      min="1"
      bind:value={participantId}
      disabled={creation.state === "submitting"}
      required
    />

    <label for="name">Name</label>
    <input
      id="name"
      name="name"
      bind:value={name}
      disabled={creation.state === "submitting"}
    />

    <button type="submit" disabled={creation.state === "submitting"}>
      {creation.state === "submitting" ? "Creating…" : "Create chat"}
    </button>

    {#if creation.state === "error"}
      <p role="alert">{creation.message}</p>
    {:else if creation.state === "created"}
      <p role="status">Created chat {creation.chatId}.</p>
    {/if}
  </form>
</main>
