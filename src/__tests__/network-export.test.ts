import { describe, it, expect, vi, beforeEach } from 'vitest';
import Graph from 'graphology';

// ── Mocks ───────────────────────────────────────────────────────────────
// Mock Tauri dialog plugin
const mockSave = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: (...args: unknown[]) => mockSave(...args),
}));

// Mock tauriCommand (used instead of plugin-fs to avoid permission issues)
const mockTauriCommand = vi.fn();
vi.mock('../composables/use-tauri-command', () => ({
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

// Mock @sigma/export-image
const mockToBlob = vi.fn();
vi.mock('@sigma/export-image', () => ({
  toBlob: (...args: unknown[]) => mockToBlob(...args),
}));

// ── Import after mocks ──────────────────────────────────────────────────
import { exportNetworkPng, exportNetworkGexf } from '../utils/network-export';

// ── Helpers ─────────────────────────────────────────────────────────────
function makeSmallGraph(): Graph {
  const g = new Graph({ type: 'undirected' });
  g.addNode('alice', { label: 'Alice', weight: 5, x: 0, y: 0, cluster: 0 });
  g.addNode('bob', { label: 'Bob', weight: 3, x: 1, y: 1, cluster: 0 });
  g.addUndirectedEdge('alice', 'bob', { weight: 2 });
  return g;
}

function makeFakeSigma() {
  return {
    getGraph: () => makeSmallGraph(),
    getCamera: () => ({ ratio: 1, x: 0.5, y: 0.5 }),
    getContainer: () => null,
  } as unknown as Parameters<typeof exportNetworkPng>[0];
}

// ── Tests ───────────────────────────────────────────────────────────────
beforeEach(() => {
  vi.clearAllMocks();
});

describe('exportNetworkPng', () => {
  it('returns false when user cancels save dialog', async () => {
    mockSave.mockResolvedValue(null);
    const result = await exportNetworkPng(makeFakeSigma());
    expect(result).toBe(false);
    expect(mockToBlob).not.toHaveBeenCalled();
    expect(mockTauriCommand).not.toHaveBeenCalled();
  });

  it('renders PNG blob, converts to base64, and writes via tauriCommand', async () => {
    mockSave.mockResolvedValue('/home/user/Pictures/network.png');
    // toBlob returns a real-ish blob
    mockToBlob.mockResolvedValue(new Blob(['fake-png-bytes'], { type: 'image/png' }));
    mockTauriCommand.mockResolvedValue(undefined);

    const result = await exportNetworkPng(makeFakeSigma(), 'my-network.png');

    expect(result).toBe(true);
    // Verify save dialog was called with correct filter
    expect(mockSave).toHaveBeenCalledWith({
      defaultPath: 'my-network.png',
      filters: [{ name: 'PNG Image', extensions: ['png'] }],
    });
    // Verify toBlob was called with label-friendly sigmaSettings
    expect(mockToBlob).toHaveBeenCalledTimes(1);
    const [_sigmaArg, opts] = mockToBlob.mock.calls[0]!;
    expect(opts).toMatchObject({
      format: 'png',
      backgroundColor: '#ffffff',
      sigmaSettings: { labelRenderedSizeThreshold: 0 },
    });
    // Verify tauriCommand was called with write_base64_to_file
    expect(mockTauriCommand).toHaveBeenCalledTimes(1);
    const [cmd, args] = mockTauriCommand.mock.calls[0]!;
    expect(cmd).toBe('write_base64_to_file');
    expect(args).toHaveProperty('path', '/home/user/Pictures/network.png');
    expect(args).toHaveProperty('data');
    expect(typeof (args as Record<string, unknown>).data).toBe('string');
  });
});

describe('exportNetworkGexf', () => {
  it('returns false when user cancels save dialog', async () => {
    mockSave.mockResolvedValue(null);
    const result = await exportNetworkGexf(makeSmallGraph());
    expect(result).toBe(false);
    expect(mockTauriCommand).not.toHaveBeenCalled();
  });

  it('serializes graph to GEXF XML and writes via tauriCommand', async () => {
    mockSave.mockResolvedValue('/home/user/Documents/network.gexf');
    mockTauriCommand.mockResolvedValue(undefined);
    const graph = makeSmallGraph();

    const result = await exportNetworkGexf(graph, 'my-network.gexf');

    expect(result).toBe(true);
    // Verify save dialog
    expect(mockSave).toHaveBeenCalledWith({
      defaultPath: 'my-network.gexf',
      filters: [{ name: 'GEXF File', extensions: ['gexf'] }],
    });
    // Verify tauriCommand called with write_text_to_file
    expect(mockTauriCommand).toHaveBeenCalledTimes(1);
    const [cmd, args] = mockTauriCommand.mock.calls[0]!;
    expect(cmd).toBe('write_text_to_file');
    expect(args).toHaveProperty('path', '/home/user/Documents/network.gexf');
    expect(args).toHaveProperty('content');

    // Verify the content is valid GEXF XML with our nodes
    const xml = (args as Record<string, unknown>).content as string;
    expect(xml).toContain('<?xml');
    expect(xml).toContain('<gexf');
    expect(xml).toContain('alice');
    expect(xml).toContain('bob');
  });

  it('GEXF output includes node labels', async () => {
    mockSave.mockResolvedValue('/tmp/test.gexf');
    mockTauriCommand.mockResolvedValue(undefined);
    const graph = makeSmallGraph();

    await exportNetworkGexf(graph);

    const [_, args] = mockTauriCommand.mock.calls[0]!;
    const xml = (args as Record<string, unknown>).content as string;
    expect(xml).toContain('Alice');
    expect(xml).toContain('Bob');
  });
});
