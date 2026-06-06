pub mod v001_initial;
pub mod v002_summary;
pub mod v003_changed_at;
pub mod v004_full_text;

pub struct Migration {
    pub version: i32,
    pub up_sql: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration { version: v001_initial::VERSION, up_sql: v001_initial::UP_SQL },
        Migration { version: v002_summary::VERSION, up_sql: v002_summary::UP_SQL },
        Migration { version: v003_changed_at::VERSION, up_sql: v003_changed_at::UP_SQL },
        Migration { version: v004_full_text::VERSION, up_sql: v004_full_text::UP_SQL },
    ]
}
