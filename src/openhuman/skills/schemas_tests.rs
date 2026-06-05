use super::*;

#[test]
fn schema_names_are_stable() {
    let list = skills_schemas("skills_list");
    assert_eq!(list.namespace, "skills");
    assert_eq!(list.function, "list");

    let read = skills_schemas("skills_read_resource");
    assert_eq!(read.namespace, "skills");
    assert_eq!(read.function, "read_resource");
}

#[test]
fn controller_lists_match_lengths() {
    assert_eq!(
        all_skills_controller_schemas().len(),
        all_skills_registered_controllers().len()
    );
}

#[test]
fn skill_summary_round_trip_minimum_fields() {
    let skill = Skill {
        name: "demo".to_string(),
        description: "desc".to_string(),
        version: "".to_string(),
        ..Default::default()
    };
    let summary: SkillSummary = skill.into();
    assert_eq!(summary.id, "demo");
    assert_eq!(summary.name, "demo");
    assert_eq!(summary.description, "desc");
}

// ── DADOU skill controller tests ───────────────────────────────────────

#[test]
fn dadou_skill_controllers_have_namespace_dadou() {
    let install = dadou_skills_schemas("dadou_skill_install");
    assert_eq!(install.namespace, "dadou");
    assert_eq!(install.function, "skill_install");

    let list = dadou_skills_schemas("dadou_skill_list");
    assert_eq!(list.namespace, "dadou");
    assert_eq!(list.function, "skill_list");

    let remove = dadou_skills_schemas("dadou_skill_remove");
    assert_eq!(remove.namespace, "dadou");
    assert_eq!(remove.function, "skill_remove");

    let audit = dadou_skills_schemas("dadou_skill_audit");
    assert_eq!(audit.namespace, "dadou");
    assert_eq!(audit.function, "skill_audit");

    let update = dadou_skills_schemas("dadou_skill_update");
    assert_eq!(update.namespace, "dadou");
    assert_eq!(update.function, "skill_update");

    let trust = dadou_skills_schemas("dadou_skill_trust_author");
    assert_eq!(trust.namespace, "dadou");
    assert_eq!(trust.function, "skill_trust_author");
}

#[test]
fn dadou_controller_lists_match_lengths() {
    assert_eq!(
        all_dadou_skills_controller_schemas().len(),
        all_dadou_skills_registered_controllers().len()
    );
}

#[test]
fn dadou_unknown_controller() {
    let unknown = dadou_skills_schemas("nonexistent");
    assert_eq!(unknown.namespace, "dadou");
    assert_eq!(unknown.function, "unknown");
}

#[test]
fn dadou_install_schema_has_url_param() {
    let schema = dadou_skills_schemas("dadou_skill_install");
    assert_eq!(schema.inputs.len(), 1);
    assert_eq!(schema.inputs[0].name, "url");
    assert!(schema.inputs[0].required);
}

#[test]
fn dadou_list_schema_has_no_inputs() {
    let schema = dadou_skills_schemas("dadou_skill_list");
    assert!(schema.inputs.is_empty());
}

#[test]
fn dadou_remove_schema_has_name_param() {
    let schema = dadou_skills_schemas("dadou_skill_remove");
    assert_eq!(schema.inputs.len(), 1);
    assert_eq!(schema.inputs[0].name, "name");
    assert!(schema.inputs[0].required);
}

#[test]
fn dadou_audit_schema_has_name_param() {
    let schema = dadou_skills_schemas("dadou_skill_audit");
    assert_eq!(schema.inputs.len(), 1);
    assert_eq!(schema.inputs[0].name, "name");
    assert!(schema.inputs[0].required);
}

#[test]
fn dadou_trust_author_schema_has_pubkey_param() {
    let schema = dadou_skills_schemas("dadou_skill_trust_author");
    assert_eq!(schema.inputs.len(), 1);
    assert_eq!(schema.inputs[0].name, "pubkey_pem");
    assert!(schema.inputs[0].required);
}
