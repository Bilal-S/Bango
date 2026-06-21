pub mod authors;
pub mod institutions;
pub mod kpis;
pub mod networks;
pub mod normalization;
pub mod productivity;
pub mod terms;

// Re-export terms functions:
pub use terms::{
    get_all_terms, get_terms_for_article, link_article_term, save_article_terms, upsert_term,
};

// Re-export authors functions:
pub use authors::{
    compute_author_metrics, compute_h_index, get_all_authors, get_author_pubs_by_year,
    get_authors_for_article, link_article_author, upsert_author,
};

// Re-export institutions functions:
pub use institutions::{
    count_unmatched_affiliations, get_institutions_by_author, insert_author_affiliation,
    upsert_institution,
};

// Re-export networks functions:
pub use networks::{
    auto_match_references_to_articles, build_citation_edges, build_coauthor_edges, delete_network,
    format_paper_label, get_citation_network_json, get_coauthor_network_json,
    get_cocitation_network_json, get_keyword_network_json, load_network, load_network_edges,
    load_network_nodes, save_network, CocitationNormalization, CocitationScope,
};

// Re-export kpis functions:
pub use kpis::{get_biblio_kpis, get_journal_year_data};

// Re-export productivity functions:
pub use productivity::{get_author_detail, get_author_productivity_kpis, get_author_rankings};

// Re-export normalization functions:
pub use normalization::{
    clear_all_biblio, clear_regeneratable_biblio, get_biblio_status, normalize_affiliations,
    normalize_authors_from_articles, normalize_terms_from_articles, run_full_normalization,
};
