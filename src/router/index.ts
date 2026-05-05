import { createRouter, createWebHashHistory } from 'vue-router';

const Dashboard = () => import('@/views/dashboard.vue');
const Placeholder = () => import('@/views/placeholder.vue');
const ImportRis = () => import('@/views/import-ris.vue');
const DedupReview = () => import('@/views/dedup-review.vue');
const TagLabelManagement = () => import('@/views/tag-label-management.vue');

const routes = [
  { path: '/', name: 'dashboard', component: Dashboard },
  {
    path: '/articles',
    name: 'articles',
    component: Placeholder,
    props: { title: 'Articles' },
  },
  { path: '/import', name: 'import', component: ImportRis },
  { path: '/dedup', name: 'dedup', component: DedupReview },
  {
    path: '/criteria',
    name: 'criteria',
    component: Placeholder,
    props: { title: 'Criteria Editor' },
  },
  {
    path: '/screening',
    name: 'screening',
    component: Placeholder,
    props: { title: 'AI Screening' },
  },
  {
    path: '/tags',
    name: 'tags',
    component: TagLabelManagement,
  },
  {
    path: '/prisma',
    name: 'prisma',
    component: Placeholder,
    props: { title: 'PRISMA Flow Diagram' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: Placeholder,
    props: { title: 'LLM Configuration' },
  },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
