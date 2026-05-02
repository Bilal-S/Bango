<script setup lang="ts">
import { ref } from 'vue';

const greeting = ref('Bango — AI-Powered Systematic Literature Review');
</script>

<template>
  <div class="app">
    <h1>{{ greeting }}</h1>
  </div>
</template>

<style scoped>
.app {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
}
</style>
