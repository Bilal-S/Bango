pub mod v001_initial;
pub mod v002_wiki_manifest;
pub mod v003_fts_sections;
pub mod v004_ai_screen_enhanced;

pub struct Migration {
    pub version: i32,
    pub up_sql: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration { version: v001_initial::VERSION, up_sql: v001_initial::UP_SQL },
        Migration { version: v002_wiki_manifest::VERSION, up_sql: v002_wiki_manifest::UP_SQL },
        Migration { version: v003_fts_sections::VERSION, up_sql: v003_fts_sections::UP_SQL },
        Migration {
            version: v004_ai_screen_enhanced::VERSION,
            up_sql: v004_ai_screen_enhanced::UP_SQL,
        },
    ]
}
