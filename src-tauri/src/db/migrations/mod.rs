pub mod v001_initial;
pub mod v002_tag_label_color;
pub mod v003_duplicate_status;

pub struct Migration {
    pub version: i32,
    pub up_sql: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration { version: v001_initial::VERSION, up_sql: v001_initial::UP_SQL },
        Migration { version: v002_tag_label_color::VERSION, up_sql: v002_tag_label_color::UP_SQL },
        Migration { version: v003_duplicate_status::VERSION, up_sql: v003_duplicate_status::UP_SQL },
    ]
}
