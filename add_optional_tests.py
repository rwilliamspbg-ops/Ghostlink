path = 'crates/ghost-link/src/main.rs'
with open(path, 'r') as f:
    content = f.read()

optional_test = """    #[test]
    fn test_detect_missing_optional_gui_python_modules() {
        let python = "python3";
        // This should pass regardless of whether huggingface_hub is installed,
        // as the function itself returns a Result<Vec<String>>.
        let result = detect_missing_optional_gui_python_modules(python);
        assert!(result.is_ok());
    }"""

content = content.replace("    #[test]\n    fn test_detect_missing_python_modules() {", optional_test + "\n\n    #[test]\n    fn test_detect_missing_python_modules() {")

with open(path, 'w') as f:
    f.write(content)
