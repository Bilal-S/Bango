use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;
use crate::models::biblio::{BiblioNetworkEdge, BiblioNetworkMeta, BiblioNetworkNode, NetworkType};

/// Save a network with its nodes and edges. Returns the network ID.
pub fn save_network(
    conn: &Connection,
    network_type: &NetworkType,
    label: &str,
    article_filter: Option<&str>,
    params_json: Option<&str>,
    nodes: &[BiblioNetworkNode],
    edges: &[BiblioNetworkEdge],
) -> Result<String, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let type_str = network_type.to_string();
    conn.execute(
        "INSERT INTO biblio_network_meta (id, network_type, label, article_filter, params_json, node_count, edge_count) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, type_str, label, article_filter, params_json, nodes.len() as i32, edges.len() as i32],
    )?;

    for node in nodes {
        let node_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_network_nodes (id, network_id, entity_id, label, weight, cluster, x, y) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                node_id, id, node.entity_id, node.label, node.weight, node.cluster, node.x, node.y
            ],
        )?;
    }

    for edge in edges {
        let edge_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![edge_id, id, edge.source_id, edge.target_id, edge.weight],
        )?;
    }

    Ok(id)
}

/// Load a network by ID.
pub fn load_network(
    conn: &Connection,
    network_id: &str,
) -> Result<Option<BiblioNetworkMeta>, AppError> {
    let result = conn
        .query_row(
            "SELECT id, network_type, label, article_filter, params_json, node_count, edge_count, created_at \
             FROM biblio_network_meta WHERE id = ?1",
            rusqlite::params![network_id],
            |row| {
                let type_str: String = row.get(1)?;
                let network_type = parse_network_type(&type_str);
                Ok(BiblioNetworkMeta {
                    id: row.get(0)?,
                    network_type,
                    label: row.get(2)?,
                    article_filter: row.get(3)?,
                    params_json: row.get(4)?,
                    node_count: row.get(5)?,
                    edge_count: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()?;
    Ok(result)
}

/// Load nodes for a network.
pub fn load_network_nodes(
    conn: &Connection,
    network_id: &str,
) -> Result<Vec<BiblioNetworkNode>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, network_id, entity_id, label, weight, cluster, x, y \
         FROM biblio_network_nodes WHERE network_id = ?1",
    )?;
    let nodes = stmt
        .query_map(rusqlite::params![network_id], |row| {
            Ok(BiblioNetworkNode {
                id: row.get(0)?,
                network_id: row.get(1)?,
                entity_id: row.get(2)?,
                label: row.get(3)?,
                weight: row.get(4)?,
                cluster: row.get(5)?,
                x: row.get(6)?,
                y: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(nodes)
}

/// Load edges for a network.
pub fn load_network_edges(
    conn: &Connection,
    network_id: &str,
) -> Result<Vec<BiblioNetworkEdge>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, network_id, source_id, target_id, weight \
         FROM biblio_network_edges WHERE network_id = ?1",
    )?;
    let edges = stmt
        .query_map(rusqlite::params![network_id], |row| {
            Ok(BiblioNetworkEdge {
                id: row.get(0)?,
                network_id: row.get(1)?,
                source_id: row.get(2)?,
                target_id: row.get(3)?,
                weight: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(edges)
}

/// Delete a network and all its nodes/edges (CASCADE).
pub fn delete_network(conn: &Connection, network_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM biblio_network_meta WHERE id = ?1", rusqlite::params![network_id])?;
    Ok(())
}

/// Helper parsing network type from string.
pub(super) fn parse_network_type(s: &str) -> NetworkType {
    match s {
        "co_authorship" => NetworkType::CoAuthorship,
        "co_occurrence" => NetworkType::CoOccurrence,
        "biblio_coupling" => NetworkType::BiblioCoupling,
        "co_citation" => NetworkType::CoCitation,
        _ => NetworkType::Citation,
    }
}
