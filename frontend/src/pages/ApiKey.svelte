<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../lib/api'
  import { showToast } from '../lib/toast'

  let info = $state<{ configured: boolean; key_prefix: string } | null>(null)
  let revealed = $state<string | null>(null)

  onMount(async () => {
    try {
      info = await api.apiKeyInfo()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  })

  async function rotate() {
    try {
      const data = await api.rotateApiKey()
      revealed = data.api_key
      info = { configured: true, key_prefix: data.api_key.slice(0, 12) }
      showToast(data.warning)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }
</script>

<div class="card stack">
  <div>
    <h2>API key</h2>
    <p class="muted">
      Sonarr-style key for automation. Send header <code>X-Api-Key</code> to endpoints under
      <code>/api/v1/*</code> and admin routes.
    </p>
  </div>
  {#if info}
    <p class="muted">
      {info.configured ? `Configured · prefix ${info.key_prefix}…` : 'No API key generated yet.'}
    </p>
  {/if}
  <button type="button" onclick={rotate}>Generate / rotate key</button>
  {#if revealed}
    <label>New API key (copy now)
      <input readonly value={revealed} />
    </label>
  {/if}
  <div class="muted">
    <p>Examples:</p>
    <pre style="white-space:pre-wrap;font-family:var(--mono);font-size:0.8rem">POST /api/v1/user
GET  /api/v1/queue
GET  /api/v1/queue/&#123;username&#125;</pre>
  </div>
</div>
