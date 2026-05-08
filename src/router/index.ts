import { createRouter, createWebHashHistory } from 'vue-router';
// Core views are statically imported — their JS is bundled into the main chunk
// and parsed immediately, so navigating to Dashboard or Articles is always instant.
import Dashboard from '@/views/dashboard.vue';
import ArticleList from '@/views/article-list.vue';

// Secondary views remain lazy — they'll be prefetched after the app is ready.
const ImportRis = () => import('@/views/import-ris.vue');
const DedupReview = () => import('@/views/dedup-review.vue');
const CriteriaEditor = () => import('@/views/criteria-editor.vue');
const LlmConfigView = () => import('@/views/llm-config.vue');
const TagLabelManagement = () => import('@/views/tag-label-management.vue');
const ScreeningProgress = () => import('@/views/screening-progress.vue');
const SummaryView = () => import('@/views/summary-view.vue');
const PrismaDiagram = () => import('@/views/prisma-diagram.vue');

const routes = [
  { path: '/', name: 'dashboard', component: Dashboard },
  {
    path: '/articles',
    name: 'articles',
    component: ArticleList,
  },
  { path: '/import', name: 'import', component: ImportRis },
  { path: '/dedup', name: 'dedup', component: DedupReview },
  { path: '/criteria', name: 'criteria', component: CriteriaEditor },
  {
    path: '/screening',
    name: 'screening',
    component: ScreeningProgress,
  },
  {
    path: '/tags',
    name: 'tags',
    component: TagLabelManagement,
  },
  {
    path: '/prisma',
    name: 'prisma',
    component: PrismaDiagram,
  },
  {
    path: '/summary',
    name: 'summary',
    component: SummaryView,
  },
  { path: '/settings', name: 'settings', component: LlmConfigView },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

// After the router is ready, prefetch the most-commonly visited lazy chunks
// in the background. WebKit caches the parsed modules so first navigation
// to these views is fast even though they are lazy.
void router.isReady().then(() => {
  void Promise.all([
    import('@/views/criteria-editor.vue'),
    import('@/views/llm-config.vue'),
    import('@/views/tag-label-management.vue'),
    import('@/views/import-ris.vue'),
    import('@/views/screening-progress.vue'),
  ]);
});

export default router;
