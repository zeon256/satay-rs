use super::{codegen, common::*};
use codegen::RootModule;
use std::fs;

#[test]
fn both_root_layouts_compile_and_decode() {
    for root_module in [RootModule::ModRs, RootModule::LibRs] {
        let temp = tempfile::tempdir().unwrap();
        write_manifest(temp.path(), &runtime_path_toml(), false, false);
        let files =
            codegen::generate_with(SIMPLE, codegen::GenerateOptions { root_module }).unwrap();
        let (directory, prefix) = match root_module {
            RootModule::ModRs => (
                temp.path().join("src/generated"),
                "pub mod generated;\nuse generated::*;\n",
            ),
            RootModule::LibRs => (temp.path().join("src"), ""),
        };
        write_generated_files(&directory, &files);
        let root = temp.path().join("src/lib.rs");
        let mut source = if root.exists() {
            fs::read_to_string(&root).unwrap()
        } else {
            prefix.to_owned()
        };
        source.push_str(r#"
#[cfg(all(test, feature = "json"))]
mod consumer {
    use super::*;
    #[test]
    fn request_encoding() {
        let parts = operations::get_user::get_user_parts(GetUserInput::<String>::new("user/42").include_details(true)).unwrap();
        assert_eq!(parts.uri, "/users/user%2F42?includeDetails=true");
        let response = satay_runtime::ResponseParts {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: b"{\"id\":\"42\",\"name\":\"Ada\",\"status\":\"active\"}".to_vec(),
        };
        let decoded: GetUserResponse = operations::get_user::decode_get_user_response(response.as_bytes()).unwrap();
        let GetUserResponse::Ok(user) = decoded else { panic!("expected user") };
        assert_eq!(user.id, "42");
        assert_eq!(user.status, UserStatus::Active);
    }
}
"#);
        fs::write(root, source).unwrap();
        run_temp_cargo(temp.path(), "test", &[], "root layout behavior");
        run_temp_cargo(
            temp.path(),
            "check",
            &["--no-default-features"],
            "root layout without features",
        );
        run_temp_cargo(
            temp.path(),
            "check",
            &["--no-default-features", "--features", "serde"],
            "root layout serde only",
        );
    }
}
