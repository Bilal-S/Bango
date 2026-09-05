<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

const emit = defineEmits<{
  fileSelected: [file: File];
  fileDropped: [path: string, name: string];
  zoteroSelected: [];
}>();
const isDragging = ref(false);

let unlisten: UnlistenFn | null = null;

onMounted(async () => {
  unlisten = await listen<{ paths: string[] }>('tauri://drag-drop', (event) => {
    isDragging.value = false;
    const path = event.payload.paths?.[0];
    if (path && isSupportedFile(path)) {
      const name = path.split(/[/\\]/).pop() || 'Unknown.ris';
      emit('fileDropped', path, name);
    }
  });

  await listen('tauri://drag-enter', () => {
    isDragging.value = true;
  });

  await listen('tauri://drag-leave', () => {
    isDragging.value = false;
  });
});

onUnmounted(() => {
  if (unlisten) unlisten();
});

function onDragOver(event: DragEvent): void {
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = 'copy';
  }
  isDragging.value = true;
}

function onDragLeave(): void {
  isDragging.value = false;
}

function onDrop(event: DragEvent): void {
  isDragging.value = false;

  let file: File | null = null;
  if (event.dataTransfer?.items && event.dataTransfer.items.length > 0) {
    for (let i = 0; i < event.dataTransfer.items.length; i++) {
      const item = event.dataTransfer.items[i];
      if (item?.kind === 'file') {
        file = item.getAsFile() ?? null;
        if (file) break;
      }
    }
  }

  if (!file && event.dataTransfer?.files?.length) {
    file = event.dataTransfer.files[0] ?? null;
  }

  if (file && isSupportedFile(file.name)) {
    emit('fileSelected', file);
  }
}

function isSupportedFile(name: string): boolean {
  const ext = name.toLowerCase();
  return ext.endsWith('.ris') || ext.endsWith('.bib') || ext.endsWith('.bibtex');
}

function onFileInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0];
  if (file) {
    emit('fileSelected', file);
  }
}
</script>

<template>
  <div
    class="drop-zone"
    :class="{ 'drop-zone--active': isDragging }"
    @dragenter.prevent="onDragOver"
    @dragover.prevent="onDragOver"
    @dragleave.prevent="onDragLeave"
    @drop.prevent="onDrop"
  >
    <div class="drop-zone__content">
      <div class="drop-zone__icon">↑</div>
      <p class="drop-zone__text">Drag and drop an RIS or BibTeX file here</p>
      <p class="drop-zone__subtext">or</p>
      <label class="drop-zone__button">
        Browse Files
        <input
          type="file"
          accept=".ris,.bib,.bibtex"
          class="drop-zone__input"
          @change="onFileInput"
        />
      </label>
      <button class="drop-zone__secondary" type="button" @click="emit('zoteroSelected')">
        Import from Zotero
      </button>
    </div>
  </div>
</template>

<style scoped>
.drop-zone {
  border: 2px dashed var(--color-outline-variant);
  border-radius: var(--radius-md);
  padding: var(--space-10) var(--space-6);
  text-align: center;
  transition: all 0.2s;
  background-color: var(--color-surface-container-low);
}

.drop-zone--active {
  border-color: var(--color-primary);
  background-color: var(--color-surface-container);
}

.drop-zone__content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
}

.drop-zone__icon {
  font-size: 32px;
  color: var(--color-outline);
}

.drop-zone__text {
  font-size: var(--font-size-body);
  color: var(--color-on-surface);
}

.drop-zone__subtext {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.drop-zone__button {
  display: inline-block;
  padding: var(--space-2) var(--space-4);
  background-color: var(--color-primary);
  color: var(--color-on-primary);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition: opacity 0.15s;
}

.drop-zone__button:hover {
  opacity: 0.9;
}

.drop-zone__input {
  display: none;
}

.drop-zone__secondary {
  display: inline-block;
  padding: var(--space-2) var(--space-4);
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition: opacity 0.15s;
}

.drop-zone__secondary:hover {
  opacity: 0.9;
}
</style>
