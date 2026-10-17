<script lang="ts">
  // On/off switch (role="switch"), labelled by the setting it controls.
  let {
    checked,
    onchange,
    label,
    disabled = false,
  }: { checked: boolean; onchange: (value: boolean) => void; label: string; disabled?: boolean } = $props()
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="switch"
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    flex-shrink: 0;
    width: 40px;
    height: 22px;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: var(--surface-3);
    transition: background-color 140ms var(--ease-out), border-color 140ms var(--ease-out);
  }
  .switch[aria-checked='true'] {
    border-color: transparent;
    background: var(--accent);
  }
  .switch:disabled {
    opacity: 0.45;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 999px;
    background: var(--text-muted);
    transition: transform 140ms var(--ease-out), background-color 140ms var(--ease-out);
  }
  .switch[aria-checked='true'] .knob {
    transform: translateX(18px);
    background: var(--on-accent);
  }
</style>
