import { createRouter, createWebHashHistory } from 'vue-router';

const Dashboard = () => import('@/views/dashboard.vue');
const ImportRis = () => import('@/views/import-ris.vue');
const DedupReview = () => import('@/views/dedup-review.vue');
const CriteriaEditor = () => import('@/views/criteria-editor.vue');
const LlmConfigView = () => import('@/views/llm-config.vue');
const TagLabelManagement = () => import('@/views/tag-label-management.vue');
const ArticleList = () => import('@/views/article-list.vue');
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

export default router;
