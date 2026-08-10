mod common;

use my_story_notes::markdown;

#[test]
fn example_project_has_no_empty_notes() {
    let project = common::fixture("example-project.mystorynotes");

    assert!(!project.notes.is_empty());
    for note in &project.notes {
        assert!(!note.name.is_empty());
        assert!(!note.source.is_empty());
    }
}

#[test]
fn empty_project_fixture_has_no_notes() {
    let project = common::fixture("empty_project.mystorynotes");

    assert!(project.notes.is_empty());
}

#[test]
fn cycle_project_fixture_links_form_a_cycle() {
    let project = common::fixture("cycle_project.mystorynotes");

    assert_eq!(project.notes.len(), 3);

    for note in &project.notes {
        let links = markdown::extract_links(&note.source);
        assert_eq!(
            links.len(),
            1,
            "each note in the cycle should link to exactly one other note"
        );
    }
}
