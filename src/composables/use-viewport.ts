import { ref, computed, onMounted, onUnmounted } from 'vue';

const width = ref(typeof window !== 'undefined' ? window.innerWidth : 1280);
const height = ref(typeof window !== 'undefined' ? window.innerHeight : 800);

function onResize(): void {
  width.value = window.innerWidth;
  height.value = window.innerHeight;
}

let listeners = 0;

export function useViewport() {
  const isSm = computed(() => width.value >= 640);
  const isMd = computed(() => width.value >= 768);
  const isLg = computed(() => width.value >= 1024);
  const isXl = computed(() => width.value >= 1280);

  const isBelowMd = computed(() => width.value < 768);
  const isBelowLg = computed(() => width.value < 1024);

  onMounted(() => {
    listeners++;
    if (listeners === 1) {
      window.addEventListener('resize', onResize);
    }
  });

  onUnmounted(() => {
    listeners--;
    if (listeners === 0) {
      window.removeEventListener('resize', onResize);
    }
  });

  return {
    width,
    height,
    isSm,
    isMd,
    isLg,
    isXl,
    isBelowMd,
    isBelowLg,
  };
}
