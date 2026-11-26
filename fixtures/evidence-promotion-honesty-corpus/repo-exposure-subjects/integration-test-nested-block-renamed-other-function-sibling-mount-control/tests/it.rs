#[test]
fn nested_renamed_other() {
    {
        use repo_exposure_integration_test_nested_block_renamed_other_function_sibling_mount_control_subject::other as discount;
        assert_eq!(discount(50), 5);
        assert_eq!(discount(49), 0);
    }
}
