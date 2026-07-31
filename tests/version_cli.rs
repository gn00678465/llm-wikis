#[test]
fn version_is_public_product_name_and_package_version() {
    let mut cmd = assert_cmd::Command::cargo_bin("llm-wikis").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout("llm-wikis 0.1.0\n");
}
