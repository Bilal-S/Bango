import { createRouter, createWebHashHistory } from 'vue-router';
// Core views statically imported for instant Dashboard/Articles navigation.
// Secondary views are lazy; prefetched after the app is ready.
// WebKit caches parsed modules so first navigation is fast despite lazy loading.
import Dashboard from '@/views/dashboard.vue';
import ArticleList from '@/views/article-list.vue';

const ImportRis = () => import('@/views/import-ris.vue');
const DedupReview = () => import('@/views/dedup-review.vue');
const CriteriaEditor = () => import('@/views/criteria-editor.vue');
const SettingsView = () => import('@/views/settings-view.vue');
const TagLabelManagement = () => import('@/views/tag-label-management.vue');
const ScreeningProgress = () => import('@/views/screening-progress.vue');
const SummaryView = () => import('@/views/summary-view.vue');
const PrismaDiagram = () => import('@/views/prisma-diagram.vue');
const BiblioDashboard = () => import('@/views/biblio-dashboard.vue');
const BiblioCoauthors = () => import('@/views/biblio-coauthors.vue');
const BiblioCitations = () => import('@/views/biblio-citations.vue');
const BiblioKeywords = () => import('@/views/biblio-keywords.vue');
const BiblioTimeline = () => import('@/views/biblio-timeline.vue');
const BiblioAuthors = () => import('@/views/biblio-authors.vue');
const BiblioCocitations = () => import('@/views/biblio-cocitations.vue');
const HelpGuide = () => import('@/views/help-guide.vue');
const ChatView = () => import('@/views/chat-view.vue');
const WikiView = () => import('@/views/wiki-view.vue');

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
  {
    path: '/bibliometrics',
    name: 'bibliometrics',
    component: BiblioDashboard,
    children: [
      {
        path: 'coauthors',
        name: 'coauthors',
        component: BiblioCoauthors,
      },
      {
        path: 'citations',
        name: 'citations',
        component: BiblioCitations,
      },
      {
        path: 'keywords',
        name: 'keywords',
        component: BiblioKeywords,
      },
      {
        path: 'timeline',
        name: 'timeline',
        component: BiblioTimeline,
      },
      {
        path: 'authors',
        name: 'authors',
        component: BiblioAuthors,
      },
      {
        path: 'cocitations',
        name: 'cocitations',
        component: BiblioCocitations,
      },
    ],
  },
  {
    path: '/chat',
    name: 'chat',
    component: ChatView,
  },
  {
    path: '/wiki',
    name: 'wiki',
    component: WikiView,
  },
  { path: '/settings', name: 'settings', component: SettingsView },
  { path: '/help', name: 'help', component: HelpGuide },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

// Prefetch most-commonly visited lazy chunks after router is ready.
void router.isReady().then(() => {
  void Promise.all([
    import('@/views/criteria-editor.vue'),
    import('@/views/settings-view.vue'),
    import('@/views/tag-label-management.vue'),
    import('@/views/import-ris.vue'),
    import('@/views/screening-progress.vue'),
    import('@/views/biblio-dashboard.vue'),
    import('@/views/biblio-coauthors.vue'),
    import('@/views/biblio-citations.vue'),
    import('@/views/biblio-keywords.vue'),
    import('@/views/biblio-timeline.vue'),
    import('@/views/biblio-authors.vue'),
    import('@/views/biblio-cocitations.vue'),
    import('@/views/chat-view.vue'),
    import('@/views/wiki-view.vue'),
  ]);
});

export default router;
