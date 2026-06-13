import Graph from 'graphology';
import louvain from 'graphology-communities-louvain';

try {
  const g = new Graph({ type: 'directed' });
  g.addNode('A');
  g.addNode('B');
  g.addDirectedEdge('A', 'B');

  console.log('Running louvain on directed graph...');
  const details = louvain.detailed(g);
  console.log('Success! Communities:', details.communities);
} catch (e) {
  console.error('Failed as expected:', e.message);
}
