pub mod terms;
pub mod authors;
pub mod institutions;
pub mod networks;
pub mod kpis;
pub mod normalization;

// Re-export terms functions:
pub use terms::{
    upsert_term, link_article_term, get_terms_for_article, save_article_terms, get_all_terms,
};

// Re-export authors functions:
pub use authors::{
    upsert_author, link_article_author, get_authors_for_article, compute_author_metrics,
    compute_h_index, get_all_authors, get_author_pubs_by_year,
};

// Re-export institutions functions:
pub use institutions::{
    upsert_institution, insert_author_affiliation, get_institutions_by_author,
    count_unmatched_affiliations,
};

// Re-export networks functions:
pub use networks::{
    save_network, load_network, load_network_nodes, load_network_edges, delete_network,
    build_coauthor_edges, get_coauthor_network_json, format_paper_label,
    auto_match_references_to_articles, build_citation_edges, get_citation_network_json,
};

// Re-export kpis functions:
pub use kpis::get_biblio_kpis;

// Re-export normalization functions:
pub use normalization::{
    normalize_authors_from_articles, normalize_terms_from_articles, normalize_affiliations,
    clear_all_biblio, clear_regeneratable_biblio, get_biblio_status,
};
