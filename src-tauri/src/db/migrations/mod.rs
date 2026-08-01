pub mod v001_initial;
pub mod v002_wiki_manifest;
pub mod v003_articles_translations;
pub mod v004_gap_analysis;
pub mod v005_audit_note_add;
pub mod v006_audit_metadata_edit;
pub mod v007_audit_clear_and_embeddings;
pub mod v008_audit_index_restore;

pub struct Migration {
    pub version: i32,
    pub up_sql: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration { version: v001_initial::VERSION, up_sql: v001_initial::UP_SQL },
        Migration { version: v002_wiki_manifest::VERSION, up_sql: v002_wiki_manifest::UP_SQL },
        Migration {
            version: v003_articles_translations::VERSION,
            up_sql: v003_articles_translations::UP_SQL,
        },
        Migration { version: v004_gap_analysis::VERSION, up_sql: v004_gap_analysis::UP_SQL },
        Migration { version: v005_audit_note_add::VERSION, up_sql: v005_audit_note_add::UP_SQL },
        Migration {
            version: v006_audit_metadata_edit::VERSION,
            up_sql: v006_audit_metadata_edit::UP_SQL,
        },
        Migration {
            version: v007_audit_clear_and_embeddings::VERSION,
            up_sql: v007_audit_clear_and_embeddings::UP_SQL,
        },
        Migration {
            version: v008_audit_index_restore::VERSION,
            up_sql: v008_audit_index_restore::UP_SQL,
        },
    ]
}
