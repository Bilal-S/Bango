pub mod v001_initial;
pub mod v002_wiki_manifest;

pub struct Migration {
    pub version: i32,
    pub up_sql: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration { version: v001_initial::VERSION, up_sql: v001_initial::UP_SQL },
        Migration { version: v002_wiki_manifest::VERSION, up_sql: v002_wiki_manifest::UP_SQL },
    ]
}
