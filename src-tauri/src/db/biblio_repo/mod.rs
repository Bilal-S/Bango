pub mod authors;
pub mod institutions;
pub mod kpis;
pub mod networks;
pub mod normalization;
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
    format_paper_label, get_citation_network_json, get_coauthor_network_json, load_network,
    load_network_edges, load_network_nodes, save_network,
};

// Re-export kpis functions:
pub use kpis::get_biblio_kpis;

// Re-export normalization functions:
pub use normalization::{
    clear_all_biblio, clear_regeneratable_biblio, get_biblio_status, normalize_affiliations,
    normalize_authors_from_articles, normalize_terms_from_articles,
};
