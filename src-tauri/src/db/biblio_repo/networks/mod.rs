//! Bibliometric network builders and serializers.
//!
//! This module is organized by network type:
//! - [`persistence`] — generic network CRUD (save/load/delete nodes & edges).
//! - [`labels`] — shared paper-label formatter.
//! - [`coauthors`] — co-authorship edge builder + JSON serializer.
//! - [`citations`] — citation edge builder + JSON serializer (incl. unmatched leaves).
//! - [`keywords`] — keyword co-occurrence JSON serializer.
//! - [`cocitation`] — on-demand co-citation computation with 4 normalization modes.

mod citations;
mod coauthors;
mod cocitation;
mod keywords;
mod labels;
mod persistence;

// Re-export the public API so callers can use `networks::foo` (or the
// flattened `biblio_repo::foo` re-export in the parent mod.rs) unchanged.
pub use citations::{
    auto_match_references_to_articles, build_citation_edges, get_citation_network_json,
};
pub use coauthors::{build_coauthor_edges, get_coauthor_network_json};
pub use cocitation::{get_cocitation_network_json, CocitationNormalization, CocitationScope};
pub use keywords::get_keyword_network_json;
pub use labels::format_paper_label;
pub use persistence::{
    delete_network, load_network, load_network_edges, load_network_nodes, save_network,
};
