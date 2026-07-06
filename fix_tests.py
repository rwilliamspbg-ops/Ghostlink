path = 'crates/ghost-link/src/main.rs'
with open(path, 'r') as f:
    content = f.read()

bad_part = """        #[test]
        fn test_detect_missing_python_modules() {
            let python = "python3";
            // Test with modules that should exist
            let missing = detect_missing_python_modules(python, &["sys", "os"]).unwrap();
            assert!(missing.is_empty());

            // Test with a module that definitely doesn't exist
            let missing =
                detect_missing_python_modules(python, &["non_existent_module_ghostlink_test"])
                    .unwrap();
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0], "non_existent_module_ghostlink_test");
        }"""

content = content.replace(bad_part, "")

new_test = """    #[test]
    fn test_detect_missing_python_modules() {
        let python = "python3";
        // Test with modules that should exist
        let missing = detect_missing_python_modules(python, &["sys", "os"]).unwrap();
        assert!(missing.is_empty());

        // Test with a module that definitely doesn't exist
        let missing =
            detect_missing_python_modules(python, &["non_existent_module_ghostlink_test"]).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], "non_existent_module_ghostlink_test");
    }"""

content = content.replace("    #[test]\n    fn rejects_invalid_input() {", new_test + "\n\n    #[test]\n    fn rejects_invalid_input() {")

with open(path, 'w') as f:
    f.write(content)
