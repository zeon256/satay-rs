use super::codegen;
use std::fs;

use super::ast::*;
use super::common::*;

#[test]
fn generated_simple_fixture_compiles_and_behaves() {
    let files = codegen::generate(SIMPLE).expect("generate simple fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!("tests/behavior/generated_simple_fixture_compiles_and_behaves/tests.rs"),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "generated crate tests");
}

#[test]
fn generated_group_views_compile_and_share_actions() {
    let files = codegen::generate(GROUPED).expect("generate grouped fixture");
    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    write_manifest(crate_dir, &runtime_path_toml(), false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!("tests/behavior/generated_group_views_compile_and_share_actions/tests.rs"),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "grouped generated crate tests");
}

#[test]
fn generated_response_name_collision_compiles_and_decodes() {
    let files = codegen::generate(RESPONSE_NAME_COLLISION).expect("generate collision fixture");

    let parts = parse_rust(find_file(&files, "psi/parts.rs"));
    let response = find_enum(&parts, "PsiOperationResponse");
    assert_eq!(
        norm(&variant(response, "Ok").fields),
        norm_str("(PsiResponse)")
    );
    assert!(!has_enum(&parts, "PsiResponse"));

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, false, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/behavior/generated_response_name_collision_compiles_and_decodes/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(
        crate_dir,
        "test",
        &[],
        "response collision generated crate tests",
    );
}

#[test]
fn generated_constrained_fixture_enforces_openapi_bounds() {
    let files = codegen::generate(CONSTRAINED).expect("generate constrained fixture");

    let types_rs = parse_rust(find_file(&files, "types.rs"));
    assert_tuple_struct(&types_rs, "Age", "u8");
    assert_attr_contains(
        &find_struct(&types_rs, "Age").attrs,
        "nutype::nutype",
        "validate(less_or_equal = 130)",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "UserName").attrs,
        "nutype::nutype",
        "validate(len_char_min = 1, len_char_max = 80)",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "UserNickname").attrs,
        "nutype::nutype",
        "validate(len_char_min = 1, len_char_max = 40)",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "UserScore").attrs,
        "nutype::nutype",
        "validate(finite, greater = 0.0, less = 1.0)",
    );
    assert_field(
        find_struct(&types_rs, "User"),
        "nickname",
        "Option<UserNickname>",
    );
    assert_attr_contains(
        &find_struct(&types_rs, "GetUserUserIdParameter").attrs,
        "nutype::nutype",
        r#"regex = "^[a-zA-Z0-9-]+$""#,
    );

    let temp = tempfile::tempdir().expect("create temp crate");
    let crate_dir = temp.path();
    let generated_dir = crate_dir.join("src/generated");

    let runtime_path = runtime_path_toml();

    write_manifest(crate_dir, &runtime_path, true, false);
    write_generated_files(&generated_dir, &files);
    write_fixture_tests(
        temp.path(),
        include_str!(
            "tests/behavior/generated_constrained_fixture_enforces_openapi_bounds/tests.rs"
        ),
    );
    let lib_contents = TEST_CRATE_LIB;
    fs::write(crate_dir.join("src/lib.rs"), lib_contents).expect("write lib");

    run_temp_cargo(crate_dir, "test", &[], "constrained generated crate tests");
    run_temp_cargo(
        crate_dir,
        "check",
        &["--no-default-features"],
        "constrained generated crate no-default check",
    );
}
