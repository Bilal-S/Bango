export function formatDate(isoString: string): string {
  return new Date(isoString).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export function formatConfidence(confidence: number | null): string {
  if (confidence === null) return '—';
  return `${Math.round(confidence * 100)}%`;
}

export function formatPriority(priority: string): string {
  return priority.charAt(0).toUpperCase() + priority.slice(1);
}

export function formatArticleCount(count: number): string {
  return `${count} article${count === 1 ? '' : 's'}`;
}
