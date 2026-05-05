use bango_lib::db::connection::create_connection;
use bango_lib::db::criteria_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::criterion::Priority;

#[test]
fn test_priority_ordering() {
    assert!(Priority::Critical > Priority::High);
    assert!(Priority::High > Priority::Standard);
    assert!(Priority::Standard > Priority::Low);
    assert!(Priority::Low > Priority::Optional);
}

#[test]
fn test_criteria_priority_in_database() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    criteria_repo::create_criterion(&conn, "inclusion", "Critical item", "critical").unwrap();
    criteria_repo::create_criterion(&conn, "inclusion", "Standard item", "standard").unwrap();
    criteria_repo::create_criterion(&conn, "exclusion", "High item", "high").unwrap();

    let all = criteria_repo::get_all_criteria(&conn).unwrap();
    assert_eq!(all.len(), 3);

    let critical = all.iter().find(|c| c.text == "Critical item").unwrap();
    assert!(matches!(critical.priority, Priority::Critical));
}

#[test]
fn test_criteria_type_filtering() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    criteria_repo::create_criterion(&conn, "inclusion", "Include ML", "standard").unwrap();
    criteria_repo::create_criterion(&conn, "inclusion", "Include AI", "high").unwrap();
    criteria_repo::create_criterion(&conn, "exclusion", "Exclude non-English", "standard").unwrap();

    let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion").unwrap();
    let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion").unwrap();
    assert_eq!(inc.len(), 2);
    assert_eq!(exc.len(), 1);
}
