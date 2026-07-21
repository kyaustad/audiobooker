<script lang="ts">
  let { name }: { name: 'queue' | 'discover' | 'approvals' | 'account' | 'settings' | 'users' | 'api' } =
    $props()
</script>

<svg class="bn-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
  {#if name === 'queue'}
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M4 6h16M4 12h10M4 18h14"
    />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M16 10.5v7l5-3.5-5-3.5z"
    />
  {:else if name === 'discover'}
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2" />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="m14.5 9.5-1.2 3.8-3.8 1.2 1.2-3.8 3.8-1.2z"
    />
  {:else if name === 'approvals'}
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2" />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="m8.5 12.5 2.5 2.5 4.5-5"
    />
  {:else if name === 'account'}
    <circle cx="12" cy="8" r="3.5" fill="none" stroke="currentColor" stroke-width="2" />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M5.5 19.5a6.5 6.5 0 0 1 13 0"
    />
  {:else if name === 'settings'}
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z"
    />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.09a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.09a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
    />
  {:else if name === 'users'}
    <circle cx="9" cy="8" r="3" fill="none" stroke="currentColor" stroke-width="2" />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M3.5 19a5.5 5.5 0 0 1 11 0"
    />
    <circle cx="17" cy="9" r="2.5" fill="none" stroke="currentColor" stroke-width="2" />
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M16.2 14.2a4.5 4.5 0 0 1 4.3 4.8"
    />
  {:else}
    <path
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M8 11V7.5a4 4 0 1 1 8 0V11"
    />
    <rect
      x="5.5"
      y="11"
      width="13"
      height="9"
      rx="2"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
    />
  {/if}
</svg>
