<script setup lang="ts">
import { onMounted, computed } from 'vue';
import { useArticleSearch } from '@/composables/use-article-search';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';

const {
  articles,
  loading,
  query,
  selectedArticle,
  auditTrail,
  showDetail,
  search,
  selectArticle,
  moveArticle,
  closeDetail,
} = useArticleSearch();

onMounted(search);

const selectedId = computed(() => selectedArticle.value?.id ?? null);

function handleUpdate(key: string, value: unknown): void {
  (query as Record<string, unknown>)[key] = value;
}

async function handleMoveArticle(id: string, newStatus: string): Promise<void> {
  await moveArticle(id, newStatus);
}
</script>

<template>
  <div class="h-full flex">
    <!-- Main content area -->
    <div
      class="flex-1 p-container-padding overflow-y-auto"
      :class="{ 'opacity-40 pointer-events-none': showDetail }"
    >
      <div class="mb-6 flex items-center justify-between">
        <h2 class="font-h1 text-h1 text-on-surface">Article Repository</h2>
      </div>

      <ArticleToolbar
        :query="query"
        :article-count="articles.length"
        @search="search"
        @update="handleUpdate"
      />

      <div v-if="loading" class="text-center py-16 text-slate-400 text-sm">Loading...</div>
      <ArticleTable
        v-else
        :articles="articles"
        :selected-id="selectedId"
        @select="selectArticle"
      />
    </div>

    <!-- Detail Panel -->
    <ArticleDetailPanel
      v-if="showDetail && selectedArticle"
      :article="selectedArticle"
      :audit-trail="auditTrail"
      @close="closeDetail"
      @move-article="handleMoveArticle"
    />
  </div>
</template>
