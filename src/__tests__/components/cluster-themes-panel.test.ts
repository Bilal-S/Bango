import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import ClusterThemesPanel from '@/components/cluster-themes-panel.vue';

type Props = InstanceType<typeof ClusterThemesPanel>['$props'];

const handlers = {
  author: vi.fn(),
  article: vi.fn(),
};

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    visible: true,
    title: 'Cluster 1 - Thematic Analysis',
    markdown: null,
    loading: false,
    error: null,
    linkHandlers: handlers,
    ...overrides,
  } as Props;
}

describe('cluster-themes-panel.vue', () => {
  it('renders_author_and_article_links_as_clickable', async () => {
    handlers.author.mockClear();
    handlers.article.mockClear();
    const markdown =
      '## Main Themes\n\n- Led by [Alice Smith](author:a-1).\n\nSee [Sugar tax paper](article:art-9).\n';
    const wrapper = mount(ClusterThemesPanel, { props: baseProps({ markdown }) });

    const authorSpan = wrapper.find('[data-protocol="author"][data-id="a-1"]');
    const articleSpan = wrapper.find('[data-protocol="article"][data-id="art-9"]');
    expect(authorSpan.exists()).toBe(true);
    expect(authorSpan.text()).toBe('Alice Smith');
    expect(articleSpan.exists()).toBe(true);
    expect(articleSpan.text()).toBe('Sugar tax paper');

    // Protocol spans carry no href (click handled via delegation).
    expect(authorSpan.attributes('href')).toBeUndefined();

    await articleSpan.trigger('click');
    expect(handlers.article).toHaveBeenCalledWith('art-9');
    await authorSpan.trigger('click');
    expect(handlers.author).toHaveBeenCalledWith('a-1');
  });

  it('renders_unknown_link_protocol_as_plain_text', async () => {
    const markdown = 'Odd [weird link](weird:xyz) and external [site](https://example.com).\n';
    const wrapper = mount(ClusterThemesPanel, { props: baseProps({ markdown }) });

    // No protocol spans for unregistered protocols.
    expect(wrapper.find('[data-protocol]').exists()).toBe(false);

    // No anchors anywhere: every unknown/external link is plain text.
    expect(wrapper.findAll('a').length).toBe(0);
    expect(wrapper.text()).toContain('weird link');
    expect(wrapper.text()).toContain('site');
  });

  it('escapes_raw_html_from_llm_output', () => {
    const markdown = 'Intro <img src=x onerror=alert(1)> and <script>alert(2)</script> end.\n';
    const wrapper = mount(ClusterThemesPanel, { props: baseProps({ markdown }) });

    // No img/script elements survive into the DOM.
    expect(wrapper.findAll('img').length).toBe(0);
    expect(wrapper.findAll('script').length).toBe(0);

    // The payloads render as escaped text, never executable markup.
    const body = wrapper.find('.cluster-themes-panel__body');
    expect(body.html()).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(body.html()).toContain('&lt;script&gt;alert(2)&lt;/script&gt;');
  });
});
