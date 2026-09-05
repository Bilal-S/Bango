use bango_lib::db::connection::create_connection;
use bango_lib::db::criteria_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::criterion::{CriterionType, Priority};

#[test]
fn test_create_and_get_aims() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let aim = criteria_repo::create_aim(&conn, "Study AI in healthcare").unwrap();
    assert_eq!(aim.text, "Study AI in healthcare");

    let aims = criteria_repo::get_all_aims(&conn).unwrap();
    assert_eq!(aims.len(), 1);
}

#[test]
fn test_delete_aim() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let aim = criteria_repo::create_aim(&conn, "Test aim").unwrap();
    criteria_repo::delete_aim(&conn, &aim.id).unwrap();
    assert!(criteria_repo::get_all_aims(&conn).unwrap().is_empty());
}

#[test]
fn test_update_aim() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let aim = criteria_repo::create_aim(&conn, "Original aim").unwrap();
    let updated = criteria_repo::update_aim(&conn, &aim.id, "Updated aim").unwrap();
    assert_eq!(updated.text, "Updated aim");
    assert_eq!(updated.id, aim.id);
}

#[test]
fn test_create_criterion_with_priority() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let criterion =
        criteria_repo::create_criterion(&conn, "inclusion", "Must be about ML", "critical")
            .unwrap();
    assert_eq!(criterion.text, "Must be about ML");
    assert!(matches!(criterion.criterion_type, CriterionType::Inclusion));
    assert!(matches!(criterion.priority, Priority::Critical));
}

#[test]
fn test_get_criteria_by_type() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    criteria_repo::create_criterion(&conn, "inclusion", "Include ML", "standard").unwrap();
    criteria_repo::create_criterion(&conn, "exclusion", "Exclude non-English", "high").unwrap();

    let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion").unwrap();
    let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion").unwrap();
    assert_eq!(inc.len(), 1);
    assert_eq!(exc.len(), 1);
}

#[test]
fn test_update_criterion() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let c = criteria_repo::create_criterion(&conn, "inclusion", "Original", "low").unwrap();
    let updated = criteria_repo::update_criterion(&conn, &c.id, "Updated", "critical").unwrap();
    assert_eq!(updated.text, "Updated");
    assert!(matches!(updated.priority, Priority::Critical));
}

#[test]
fn test_delete_criterion() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let c = criteria_repo::create_criterion(&conn, "exclusion", "To delete", "standard").unwrap();
    criteria_repo::delete_criterion(&conn, &c.id).unwrap();
    assert!(criteria_repo::get_all_criteria(&conn).unwrap().is_empty());
}
