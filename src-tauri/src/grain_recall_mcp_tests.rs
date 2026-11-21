use super::*;

fn launch() -> Launch {
    Launch::new(
        Path::new(r"C:\Program Files\Grain\grain-recall-mcp.exe"),
        Path::new(r"C:\Users\me\AppData\Roaming\com.grain.app"),
    )
}

fn places(root: &Path) -> Places {
    Places {
        home: root.join("home"),
        config: root.join("roaming"),
        local: root.join("local"),
    }
}

#[test]
fn json_connect_keeps_other_servers_and_settings_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("claude_desktop_config.json");
    fs::write(
        &file,
        r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#,
    )
    .unwrap();
    assert_eq!(json_state(&file, &launch()), McpClientState::NotConnected);

    json_connect(&file, &launch()).unwrap();
    json_connect(&file, &launch()).unwrap();
    let root: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(root["theme"], "dark");
    assert_eq!(root["mcpServers"]["other"]["command"], "x");
    assert_eq!(
        root["mcpServers"][SERVER_NAME]["args"][1],
        r"C:\Users\me\AppData\Roaming\com.grain.app"
    );
    // Key order is preserved: the user's keys stay where they were.
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.find("\"theme\"").unwrap() < text.find("\"mcpServers\"").unwrap());
    assert!(text.find("\"other\"").unwrap() < text.find(SERVER_NAME).unwrap());
    assert_eq!(json_state(&file, &launch()), McpClientState::Connected);

    let moved = Launch::new(
        Path::new(r"D:\Grain\grain-recall-mcp.exe"),
        Path::new(r"D:\data"),
    );
    assert_eq!(json_state(&file, &moved), McpClientState::NeedsUpdate);

    assert!(json_disconnect(&file).unwrap());
    assert!(!json_disconnect(&file).unwrap());
    let root: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert!(root["mcpServers"].get(SERVER_NAME).is_none());
    assert_eq!(root["mcpServers"]["other"]["command"], "x");
}

#[test]
fn json_connect_creates_a_missing_file_and_refuses_one_it_cannot_parse() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("new").join("mcp.json");
    json_connect(&fresh, &launch()).unwrap();
    assert_eq!(json_state(&fresh, &launch()), McpClientState::Connected);

    let commented = dir.path().join("settings.json");
    let original = "{ // my comment\n \"a\": 1 }";
    fs::write(&commented, original).unwrap();
    assert_eq!(
        json_state(&commented, &launch()),
        McpClientState::Unreadable
    );
    assert!(json_connect(&commented, &launch()).is_err());
    assert!(json_disconnect(&commented).is_err());
    assert_eq!(fs::read_to_string(&commented).unwrap(), original);

    let odd = dir.path().join("odd.json");
    fs::write(&odd, r#"{"mcpServers": []}"#).unwrap();
    assert!(json_connect(&odd, &launch()).is_err());

    let bom = dir.path().join("bom.json");
    fs::write(&bom, "\u{feff}{\"x\":1}").unwrap();
    json_connect(&bom, &launch()).unwrap();
    assert_eq!(json_state(&bom, &launch()), McpClientState::Connected);
}

#[test]
fn toml_connect_keeps_the_rest_of_codex_config() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.toml");
    let original = "# my settings\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
    fs::write(&file, original).unwrap();
    assert_eq!(toml_state(&file, &launch()), McpClientState::NotConnected);

    toml_connect(&file, &launch()).unwrap();
    toml_connect(&file, &launch()).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("# my settings\nmodel = \"o4\""));
    assert!(text.contains("[mcp_servers.other]"));
    assert!(text.contains("[mcp_servers.grain-recall]"));
    assert_eq!(text.matches("grain-recall]").count(), 1);
    assert_eq!(toml_state(&file, &launch()), McpClientState::Connected);
    let moved = Launch::new(Path::new("/opt/grain-recall-mcp"), Path::new("/data"));
    assert_eq!(toml_state(&file, &moved), McpClientState::NeedsUpdate);

    assert!(toml_disconnect(&file).unwrap());
    let text = fs::read_to_string(&file).unwrap();
    assert!(!text.contains("grain-recall"));
    assert!(text.contains("[mcp_servers.other]"));

    let fresh = dir.path().join("fresh.toml");
    toml_connect(&fresh, &launch()).unwrap();
    let text = fs::read_to_string(&fresh).unwrap();
    assert!(
        !text.contains("[mcp_servers]\n"),
        "no empty parent table: {text}"
    );

    let broken = dir.path().join("broken.toml");
    fs::write(&broken, "model = ").unwrap();
    assert_eq!(toml_state(&broken, &launch()), McpClientState::Unreadable);
    assert!(toml_connect(&broken, &launch()).is_err());
}

#[test]
fn clients_are_not_installed_until_their_folder_exists() {
    let dir = tempfile::tempdir().unwrap();
    let p = places(dir.path());
    for client in [
        McpClient::ClaudeDesktop,
        McpClient::GeminiCli,
        McpClient::Cursor,
        McpClient::Codex,
    ] {
        assert_eq!(
            client_status(&p, client, &launch()).state,
            McpClientState::NotInstalled,
            "{client:?}"
        );
    }
    fs::create_dir_all(p.home.join(".gemini")).unwrap();
    let gemini = client_status(&p, McpClient::GeminiCli, &launch());
    assert_eq!(gemini.state, McpClientState::NotConnected);
    connect(&p, McpClient::GeminiCli, &launch()).unwrap();
    assert_eq!(
        client_status(&p, McpClient::GeminiCli, &launch()).state,
        McpClientState::Connected
    );
    assert!(disconnect(&p, McpClient::GeminiCli).unwrap());
}

#[test]
fn claude_desktop_prefers_the_store_package_folder() {
    let dir = tempfile::tempdir().unwrap();
    let p = places(dir.path());
    assert_eq!(p.claude_desktop_dir(), p.config.join("Claude"));
    let packaged = p
        .local
        .join("Packages")
        .join("Claude_pzs8sxrjxfjjc")
        .join("LocalCache")
        .join("Roaming")
        .join("Claude");
    fs::create_dir_all(&packaged).unwrap();
    assert_eq!(p.claude_desktop_dir(), packaged);
}

#[test]
fn claude_code_command_quotes_paths_with_spaces() {
    assert_eq!(
        claude_code_command(&launch()),
        r#"claude mcp add --scope user grain-recall -- "C:\Program Files\Grain\grain-recall-mcp.exe" --data-dir C:\Users\me\AppData\Roaming\com.grain.app"#
    );
}

#[test]
fn json_edits_keep_nested_order_and_values_and_print_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("c.json");
    fs::write(
        &file,
        r#"{"z":1,"prefs":{"b":true,"a":[2.5,null,"s"]},"big":18446744073709551615}"#,
    )
    .unwrap();
    json_connect(&file, &launch()).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text
        .starts_with("{\n  \"z\": 1,\n  \"prefs\": {\n    \"b\": true,\n    \"a\": [\n      2.5,"));
    assert!(text.contains("\"big\": 18446744073709551615"));
    assert!(text.contains("\n  \"mcpServers\": {\n    \"grain-recall\": {\n      \"command\": "));
    assert!(json_disconnect(&file).unwrap());
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"mcpServers\": {}"));
}
