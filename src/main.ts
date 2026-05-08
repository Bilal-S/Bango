import { createApp } from 'vue';
import { createPinia } from 'pinia';
import router from './router';
import App from './App.vue';
import './styles/base.css';
import { useArticlesStore } from './stores/articles';
import { useCriteriaStore } from './stores/criteria';
import { useTagsStore } from './stores/tags';
import { useLabelsStore } from './stores/labels';
import { useLlmConfigStore } from './stores/llm-config';
import { useAuditStore } from './stores/audit';
import { useScreeningStore } from './stores/screening';

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount('#app');

// Bootstrap all stores in parallel after mount.
// This pre-warms every Pinia store with data from the DB so that
// navigating to any view is instant — no per-view onMounted IPC wait.
void Promise.all([
  useArticlesStore().fetchIfNeeded(),
  useCriteriaStore().fetchIfNeeded(),
  useTagsStore().fetchIfNeeded(),
  useLabelsStore().fetchIfNeeded(),
  useLlmConfigStore().fetchIfNeeded(),
  useAuditStore().fetchIfNeeded(),
  useScreeningStore().fetchIfNeeded(),
]);
