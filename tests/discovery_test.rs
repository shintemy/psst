use psst::data_sources::discovery::discover_tools;

#[test]
fn test_discover_finds_claude_if_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let claude_dir = home.join(".claude").join("projects").join("test");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("conversation.jsonl"), "{}").unwrap();
    let tools = discover_tools(home.to_str().unwrap());
    assert!(tools.contains(&"claude".to_string()));
}

#[test]
fn test_discover_returns_empty_for_clean_home() {
    let dir = tempfile::tempdir().unwrap();
    let tools = discover_tools(dir.path().to_str().unwrap());
    assert!(tools.is_empty());
}
