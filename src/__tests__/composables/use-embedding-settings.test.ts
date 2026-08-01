import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useEmbeddingSettings } from '@/composables/use-embedding-settings';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: vi.fn(() => true),
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

describe('useEmbeddingSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads the modelOverride from get_embedding_status', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({
      status: 'enabled',
      model: 'text-embedding-3-small',
      dimensions: 1536,
      modelOverride: 'text-embedding-3-large',
    });

    const { modelOverride, load } = useEmbeddingSettings();
    await load();

    expect(tauriCommand).toHaveBeenCalledWith('get_embedding_status');
    expect(modelOverride.value).toBe('text-embedding-3-large');
  });

  it('defaults modelOverride to empty string when backend returns null', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({
      status: 'unknown',
      model: '',
      dimensions: 0,
      modelOverride: null,
    });

    const { modelOverride, load } = useEmbeddingSettings();
    await load();

    expect(modelOverride.value).toBe('');
  });

  it('defaults modelOverride to empty string when modelOverride is undefined', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({
      status: 'unknown',
      model: '',
      dimensions: 0,
    });

    const { modelOverride, load } = useEmbeddingSettings();
    await load();

    expect(modelOverride.value).toBe('');
  });

  it('gracefully handles a read failure (leaves the field empty)', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('cmd failed'));

    const { modelOverride, load } = useEmbeddingSettings();
    await load();

    expect(modelOverride.value).toBe('');
  });

  it('saves a non-empty value via set_embedding_model_override', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(undefined);

    const { modelOverride, save } = useEmbeddingSettings();
    await save('  nomic-embed-text  ');

    expect(tauriCommand).toHaveBeenCalledWith('set_embedding_model_override', {
      value: 'nomic-embed-text',
    });
    expect(modelOverride.value).toBe('nomic-embed-text');
  });

  it('clears the override (passes null) when saving an empty/whitespace value', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(undefined);

    const { modelOverride, save } = useEmbeddingSettings();
    await save('   ');

    expect(tauriCommand).toHaveBeenCalledWith('set_embedding_model_override', {
      value: null,
    });
    expect(modelOverride.value).toBe('');
  });

  it('toggles saving flag around the save call', async () => {
    // Use a deferred promise so we can assert `saving` is true mid-flight,
    // then resolve to let the save complete.
    const deferred: { resolve: ((value: unknown) => void) | null } = { resolve: null };
    vi.mocked(tauriCommand).mockReturnValue(
      new Promise<unknown>((resolve) => {
        deferred.resolve = resolve;
      })
    );

    const { saving, save } = useEmbeddingSettings();
    const promise = save('text-embedding-3-large');
    expect(saving.value).toBe(true);

    deferred.resolve?.(undefined);
    await promise;

    expect(saving.value).toBe(false);
  });
});
