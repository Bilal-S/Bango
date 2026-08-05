//! Seed templates for `wiki-root/templates/`. Reference skeletons the LLM follows when
//! generating pages. Also useful for users adding pages by hand. Written once; overwritten on `wiki_init`.

use std::path::Path;

use crate::error::AppError;

/// A template definition: `(filename, content)`.
const TEMPLATES: &[(&str, &str)] = &[
    ("concept.md", CONCEPT_TEMPLATE),
    ("method.md", METHOD_TEMPLATE),
    ("synthesis.md", SYNTHESIS_TEMPLATE),
    ("author.md", AUTHOR_TEMPLATE),
    ("source.md", SOURCE_TEMPLATE),
];

/// Write all seed templates into `wiki-root/templates/`. Idempotent: overwrites existing.
pub fn write_all(templates_dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(templates_dir).map_err(|e| {
        AppError::Import(format!(
            "Failed to create templates dir '{}': {}",
            templates_dir.display(),
            e
        ))
    })?;
    for (name, content) in TEMPLATES {
        let path = templates_dir.join(name);
        std::fs::write(&path, content).map_err(|e| {
            AppError::Import(format!("Failed to write template '{}': {}", path.display(), e))
        })?;
    }
    Ok(())
}

const CONCEPT_TEMPLATE: &str = "\
---\n\
id: <uuid>\n\
title: \"<Concept Name>\"\n\
type: concept\n\
slug: <kebab-case-slug>\n\
summary: \"<1-2 sentence digest used for search and token-budgeted chat>.\"\n\
created: <ISO-8601>\n\
updated: <ISO-8601>\n\
status: draft\n\
source_articles: [\"<article-uuid>\"]\n\
tags: []\n\
links: [\"[[related-concept]]\"]\n\
 content_source: <full_text|ai_summary|abstract>\n\
 llm_model: <model-id>\n\
 ---\n\
 \n\
 <Opening paragraph defining the concept and why it matters to the review.>\n\
\n\
## Evidence\n\
\n\
- Finding from [[author-or-article]] [^art-<article-id>]\n\
- Contrasting finding [^art-<article-id>]\n\
\n\
## Related\n\
\n\
- [[related-concept-1]]\n\
- [[related-concept-2]]\n\
\n\
[^art-<article-id>]: /raw/<article-id>.md\n\
";

const METHOD_TEMPLATE: &str = "\
---\n\
id: <uuid>\n\
title: \"<Method Name>\"\n\
type: method\n\
slug: <kebab-case-slug>\n\
summary: \"<1-2 sentence digest: N articles use this method>.\"\n\
created: <ISO-8601>\n\
updated: <ISO-8601>\n\
status: draft\n\
source_articles: [\"<article-uuid>\"]\n\
tags: []\n\
links: [\"[[related-method]]\"]\n\
 content_source: metadata\n\
 llm_model: <model-id>\n\
 ---\n\
 \n\
 <Opening paragraph describing the method and its relevance to the review.>\n\
\n\
## Relevant Studies\n\
\n\
- [[article-uuid]] [^art-<article-id>]\n\
\n\
## Related Methods\n\
\n\
- [[related-method-1]]\n\
- [[related-method-2]]\n\
\n\
[^art-<article-id>]: /raw/<article-id>.md\n\
";

const SYNTHESIS_TEMPLATE: &str = "\
---\n\
id: <uuid>\n\
title: \"<Synthesis Title>\"\n\
type: synthesis\n\
slug: <kebab-case-slug>\n\
summary: \"<1-2 sentence digest of the cross-cutting theme or section>.\"\n\
created: <ISO-8601>\n\
updated: <ISO-8601>\n\
status: draft\n\
source_articles: [\"<article-uuid>\"]\n\
tags: []\n\
links: [\"[[concept]]\", \"[[method]]\"]\n\
 content_source: <full_text|ai_summary|abstract>\n\
 llm_model: <model-id>\n\
 ---\n\
 \n\
 <Opening paragraph framing the cross-cutting theme, study aspect, or section\n\
that connects multiple sources.>\n\
\n\
## Summary\n\
\n\
<Synthesis of the evidence across the relevant sources.>\n\
\n\
## Key Insights\n\
\n\
- Insight from [[article-or-author]] [^art-<article-id>]\n\
- Contrasting insight [^art-<article-id>]\n\
\n\
## Related\n\
\n\
- [[concept-1]]\n\
- [[method-1]]\n\
\n\
[^art-<article-id>]: /raw/<article-id>.md\n\
";

const AUTHOR_TEMPLATE: &str = "\
---\n\
id: <uuid>\n\
title: \"<Author Display Name>\"\n\
type: author\n\
slug: <kebab-case-slug>\n\
summary: \"<1-2 sentence profile summary>.\"\n\
created: <ISO-8601>\n\
updated: <ISO-8601>\n\
status: draft\n\
source_articles: [\"<article-uuid>\"]\n\
tags: []\n\
links: [\"[[co-author]]\", \"[[concept]]\"]\n\
 content_source: metadata\n\
 llm_model: <model-id>\n\
 ---\n\
 \n\
 <Affiliation and role. Derived from biblio_authors + article affiliations.>\n\
\n\
## Contributions\n\
\n\
- [[concept]] [^art-<article-id>]\n\
\n\
## Co-authors\n\
\n\
- [[co-author-1]]\n\
\n\
[^art-<article-id>]: /raw/<article-id>.md\n\
";

const SOURCE_TEMPLATE: &str = "\
---\n\
id: <uuid>\n\
title: \"<Article Title>\"\n\
type: source\n\
slug: <kebab-case-slug>\n\
summary: \"<1-2 sentence digest of the article>.\"\n\
created: <ISO-8601>\n\
updated: <ISO-8601>\n\
status: draft\n\
source_articles: [\"<article-uuid>\"]\n\
authors: [\"<Author A>\", \"<Author B>\"]\n\
year: <YYYY>\n\
journal: \"<Journal>\"\n\
doi: \"<doi>\"\n\
keywords: []\n\
tags: []\n\
labels: []\n\
links: [\"[[concept]]\"]\n\
 content_source: <full_text|ai_summary|abstract>\n\
 llm_model: <model-id>\n\
 ---\n\
 \n\
 Authors: <Author A; Author B>  |  Year: <YYYY>  |  Journal: <Journal>\n\
\n\
## Abstract / Summary\n\
\n\
<Full text, AI summary, or abstract depending on content_source.>\n\
\n\
## Concepts\n\
\n\
- [[concept-1]]\n\
\n\
[^art-<article-id>]: /raw/<article-id>.md\n\
";

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_all_creates_all_templates() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("templates");
        write_all(&dir).unwrap();
        for (name, _) in TEMPLATES {
            assert!(dir.join(name).exists(), "missing template: {name}");
        }
    }

    #[test]
    fn templates_contain_required_frontmatter_fields() {
        // Phase 4 lint will enforce these; ensure the seeds are already valid.
        for (name, content) in TEMPLATES {
            assert!(content.contains("id:"), "{name} missing id");
            assert!(content.contains("type:"), "{name} missing type");
            assert!(content.contains("slug:"), "{name} missing slug");
            assert!(content.contains("summary:"), "{name} missing summary");
            assert!(content.contains("status:"), "{name} missing status");
            assert!(content.contains("source_articles:"), "{name} missing source_articles");
            assert!(content.contains("links:"), "{name} missing links");
            assert!(content.contains("content_source:"), "{name} missing content_source");
        }
    }

    #[test]
    fn templates_have_no_em_dashes() {
        for (name, content) in TEMPLATES {
            assert!(!content.contains('\u{2014}'), "{name} contains em dash");
            assert!(!content.contains('\u{2013}'), "{name} contains en dash");
        }
    }

    #[test]
    fn write_all_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("templates");
        write_all(&dir).unwrap();
        write_all(&dir).unwrap();
        // Still exactly the expected files.
        let mut count = 0;
        for (name, _) in TEMPLATES {
            if dir.join(name).exists() {
                count += 1;
            }
        }
        assert_eq!(count, TEMPLATES.len());
    }
}
