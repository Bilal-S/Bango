<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import SettingsProviderCard from '@/components/settings/settings-provider-card.vue';
import SettingsBangoAiCard from '@/components/settings/settings-bango-ai-card.vue';
import BangoAiConsentDialog from '@/components/settings/bango-ai-consent-dialog.vue';
import { useBangoAi } from '@/composables/use-bango-ai';

/**
 * Provider section wrapper: the backend selection header plus the existing
 * AI Provider card (always mounted) and the Bango AI panel when selected.
 * The selection is radio-style, mirroring the Embeddings card; choosing
 * Bango AI before components are ready opens the consent dialog, and
 * activation persists only after the install self-test succeeds.
 */
const { backend, status, switching, installing, error, load, selectBackend, install } =
  useBangoAi();

const showConsent = ref(false);

const consentModel = computed(() => status.value?.model ?? 'Qwen3.5 2B');
const consentBytes = computed(() => status.value?.downloadBytes ?? 0);
const consentLicense = computed(() => status.value?.license ?? 'MIT');
const consentLicenseUrl = computed(() => status.value?.licenseUrl ?? '');

onMounted(load);

async function onSelectBangoAi(): Promise<void> {
  if (backend.value === 'bango_ai' || installing.value) return;
  if (status.value?.state === 'ready') {
    await selectBackend('bango_ai');
    return;
  }
  showConsent.value = true;
}

async function onConsentConfirm(): Promise<void> {
  showConsent.value = false;
  try {
    // The install activates (persists bango_ai) only after the self-test.
    await install(true);
  } catch {
    // Error surfaced by the composable.
  }
}
</script>

<template>
  <section class="ai-section" aria-label="AI backend selection">
    <h2 class="ai-section__title">Choose how Bango runs AI</h2>
    <div class="ai-section__options">
      <label class="ai-section__option" :class="{ 'is-active': backend === 'configured_provider' }">
        <input
          type="radio"
          name="llm-backend"
          value="configured_provider"
          :checked="backend === 'configured_provider'"
          :disabled="switching"
          @change="selectBackend('configured_provider')"
        />
        <span>
          <strong>Configured Provider</strong>
          <small>Uses your selected AI provider (cloud or a local server you run).</small>
        </span>
      </label>
      <label
        class="ai-section__option"
        :class="{ 'is-active': backend === 'bango_ai' || installing }"
      >
        <input
          type="radio"
          name="llm-backend"
          value="bango_ai"
          :checked="backend === 'bango_ai' || installing"
          :disabled="switching"
          @change="onSelectBangoAi"
        />
        <span>
          <strong>Bango AI</strong>
          <small>Runs on this computer. No API key or usage charges.</small>
        </span>
      </label>
    </div>

    <SettingsProviderCard v-if="backend === 'configured_provider' && !installing" />

    <!-- The card also mounts during a consent-triggered install: the backend
         stays configured_provider until the self-test succeeds, but the
         progress UI must be visible from the first downloaded byte. The
         provider configuration hides while Bango AI is selected or installing
         so the switch is visibly a switch. -->
    <SettingsBangoAiCard
      v-if="backend === 'bango_ai' || installing"
      @setup="showConsent = true"
      @switch-provider="selectBackend('configured_provider')"
    />

    <!-- Install/setup errors stay visible when the card is not mounted. -->
    <p v-if="error && backend !== 'bango_ai'" class="ai-section__error">{{ error }}</p>

    <BangoAiConsentDialog
      v-if="showConsent"
      :model="consentModel"
      :download-bytes="consentBytes"
      :license="consentLicense"
      :license-url="consentLicenseUrl"
      @confirm="onConsentConfirm"
      @cancel="showConsent = false"
    />
  </section>
</template>

<style scoped>
.ai-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.ai-section__title {
  font-size: 1rem;
  font-weight: 600;
  margin: 0;
}
.ai-section__error {
  color: #b91c1c;
  font-size: 0.85rem;
  margin: 0;
}
.ai-section__options {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.75rem;
}
@media (max-width: 767px) {
  .ai-section__options {
    grid-template-columns: 1fr;
  }
}
.ai-section__option {
  display: flex;
  gap: 0.6rem;
  align-items: flex-start;
  border: 1px solid var(--color-outline-variant, #d7d7e0);
  border-radius: 0.75rem;
  padding: 0.75rem;
  cursor: pointer;
}
.ai-section__option.is-active {
  border-color: var(--color-primary, #4f46e5);
  box-shadow: 0 0 0 1px var(--color-primary, #4f46e5);
}
.ai-section__option span {
  display: flex;
  flex-direction: column;
}
.ai-section__option small {
  color: var(--color-on-surface-variant, #464555);
}
</style>
