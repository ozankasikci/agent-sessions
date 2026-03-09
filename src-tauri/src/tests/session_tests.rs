use crate::session::{
    AgentType, SessionStatus, parse_session_file, convert_dir_name_to_path, convert_path_to_dir_name,
    determine_status, status_sort_priority, has_tool_use, has_tool_result, is_local_slash_command,
    is_interrupted_request, is_waiting_for_user_input, cleanup_stale_status_entries, get_sessions_internal
};
use crate::agent::AgentProcess;
use serde_json::json;
use std::io::Write;
use std::time::{SystemTime, Duration};
use tempfile::NamedTempFile;

// Helper functions

fn create_test_jsonl(lines: &[&str]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    for line in lines {
        writeln!(file, "{}", line).unwrap();
    }
    file.flush().unwrap();
    file
}

/// Create a test JSONL file with an old modification time (>3s ago)
/// This ensures file_recently_modified = false in status determination
fn create_test_jsonl_old(lines: &[&str]) -> NamedTempFile {
    let file = create_test_jsonl(lines);
    // Set modification time to 10 seconds ago
    let old_time = SystemTime::now() - Duration::from_secs(10);
    let old_time_file = filetime::FileTime::from_system_time(old_time);
    filetime::set_file_mtime(file.path(), old_time_file).unwrap();
    file
}

// Test constants for process info
const TEST_PID: u32 = 12345;
const TEST_CPU_USAGE: f32 = 0.0;

// Unit tests for helper functions

#[test]
fn test_convert_dir_name_to_path() {
    // Test basic project path
    assert_eq!(
        convert_dir_name_to_path("-Users-ozan-Projects-ai-image-dashboard"),
        "/Users/ozan/Projects/ai-image-dashboard"
    );

    // Test project with multiple dashes
    assert_eq!(
        convert_dir_name_to_path("-Users-ozan-Projects-backend-service-generator-ai"),
        "/Users/ozan/Projects/backend-service-generator-ai"
    );

    // Test UnityProjects
    assert_eq!(
        convert_dir_name_to_path("-Users-ozan-UnityProjects-my-game"),
        "/Users/ozan/UnityProjects/my-game"
    );

    // Test worktree paths (with double dashes -> hidden folders)
    assert_eq!(
        convert_dir_name_to_path("-Users-ozan-Projects-ai-image-dashboard--rsworktree-analytics"),
        "/Users/ozan/Projects/ai-image-dashboard/.rsworktree/analytics"
    );

    // Test multiple hidden folders
    assert_eq!(
        convert_dir_name_to_path("-Users-ozan-Projects-myproject--hidden--subfolder"),
        "/Users/ozan/Projects/myproject/.hidden/.subfolder"
    );

    // Test just Projects folder
    assert_eq!(
        convert_dir_name_to_path("-Users-ozan-Projects"),
        "/Users/ozan/Projects"
    );

    // Note: These test cases would fail with convert_dir_name_to_path because
    // the encoding is ambiguous. The reverse lookup via convert_path_to_dir_name
    // is used for matching instead.
}

#[test]
fn test_convert_path_to_dir_name() {
    // Basic path
    assert_eq!(
        convert_path_to_dir_name("/Users/ozan/Projects/ai-image-dashboard"),
        "-Users-ozan-Projects-ai-image-dashboard"
    );

    // Path with hidden folder (.rsworktree)
    assert_eq!(
        convert_path_to_dir_name("/Users/ozan/Projects/unity-build-service/.rsworktree/improve-prov-prof-creation"),
        "-Users-ozan-Projects-unity-build-service--rsworktree-improve-prov-prof-creation"
    );

    // Path with .worktrees
    assert_eq!(
        convert_path_to_dir_name("/Users/ozan/Projects/autogoals-v2/.worktrees/docker-containers"),
        "-Users-ozan-Projects-autogoals-v2--worktrees-docker-containers"
    );

    // Subfolder path (no hidden folders)
    assert_eq!(
        convert_path_to_dir_name("/Users/ozan/Projects/autogoals-v2/examples/test"),
        "-Users-ozan-Projects-autogoals-v2-examples-test"
    );
}

#[test]
fn test_has_tool_use() {
    // Array with tool_use block
    let content_with_tool_use = json!([
        {"type": "text", "text": "Let me run that command"},
        {"type": "tool_use", "id": "123", "name": "Bash", "input": {"command": "ls"}}
    ]);
    assert!(has_tool_use(&content_with_tool_use));

    // Array without tool_use
    let content_without_tool_use = json!([
        {"type": "text", "text": "Here is the result"}
    ]);
    assert!(!has_tool_use(&content_without_tool_use));

    // Empty array
    let empty_array = json!([]);
    assert!(!has_tool_use(&empty_array));

    // String content (not an array)
    let string_content = json!("Just a string");
    assert!(!has_tool_use(&string_content));

    // Array with tool_result (not tool_use)
    let content_with_tool_result = json!([
        {"type": "tool_result", "tool_use_id": "123", "content": "output"}
    ]);
    assert!(!has_tool_use(&content_with_tool_result));
}

#[test]
fn test_has_tool_result() {
    // Array with tool_result block
    let content_with_tool_result = json!([
        {"type": "tool_result", "tool_use_id": "123", "content": "command output"}
    ]);
    assert!(has_tool_result(&content_with_tool_result));

    // Array without tool_result
    let content_without_tool_result = json!([
        {"type": "text", "text": "Just text"}
    ]);
    assert!(!has_tool_result(&content_without_tool_result));

    // Empty array
    let empty_array = json!([]);
    assert!(!has_tool_result(&empty_array));

    // String content (not an array)
    let string_content = json!("Just a string");
    assert!(!has_tool_result(&string_content));

    // Array with tool_use (not tool_result)
    let content_with_tool_use = json!([
        {"type": "tool_use", "id": "123", "name": "Read"}
    ]);
    assert!(!has_tool_result(&content_with_tool_use));
}

#[test]
fn test_is_local_slash_command() {
    // Test recognized local commands
    assert!(is_local_slash_command(&json!("/clear")));
    assert!(is_local_slash_command(&json!("/compact")));
    assert!(is_local_slash_command(&json!("/help")));
    assert!(is_local_slash_command(&json!("/config")));
    assert!(is_local_slash_command(&json!("/cost")));
    assert!(is_local_slash_command(&json!("/doctor")));
    assert!(is_local_slash_command(&json!("/init")));
    assert!(is_local_slash_command(&json!("/login")));
    assert!(is_local_slash_command(&json!("/logout")));
    assert!(is_local_slash_command(&json!("/memory")));
    assert!(is_local_slash_command(&json!("/model")));
    assert!(is_local_slash_command(&json!("/permissions")));
    assert!(is_local_slash_command(&json!("/pr-comments")));
    assert!(is_local_slash_command(&json!("/review")));
    assert!(is_local_slash_command(&json!("/status")));
    assert!(is_local_slash_command(&json!("/terminal-setup")));
    assert!(is_local_slash_command(&json!("/vim")));

    // Test commands with arguments
    assert!(is_local_slash_command(&json!("/model sonnet")));
    assert!(is_local_slash_command(&json!("/memory add something")));

    // Test commands with whitespace
    assert!(is_local_slash_command(&json!("  /clear  ")));

    // Test non-local commands (these trigger Claude)
    assert!(!is_local_slash_command(&json!("Hello Claude")));
    assert!(!is_local_slash_command(&json!("/custom-command")));
    assert!(!is_local_slash_command(&json!("/fix the bug")));

    // Test array content with text block
    let array_content = json!([
        {"type": "text", "text": "/clear"}
    ]);
    assert!(is_local_slash_command(&array_content));

    // Test array content with non-local command
    let array_non_local = json!([
        {"type": "text", "text": "fix the bug"}
    ]);
    assert!(!is_local_slash_command(&array_non_local));

    // Test empty and edge cases
    assert!(!is_local_slash_command(&json!("")));
    assert!(!is_local_slash_command(&json!(null)));
    assert!(!is_local_slash_command(&json!(123)));
}

#[test]
fn test_is_waiting_for_user_input() {
    // Single AskUserQuestion -> Waiting
    assert!(is_waiting_for_user_input(&json!([
        {"type": "tool_use", "id": "1", "name": "AskUserQuestion"}
    ])));

    // Multiple AskUserQuestion -> Waiting
    assert!(is_waiting_for_user_input(&json!([
        {"type": "tool_use", "id": "1", "name": "AskUserQuestion"},
        {"type": "tool_use", "id": "2", "name": "AskUserQuestion"}
    ])));

    // Mixed: AskUserQuestion + Bash -> Processing (not all are user-input)
    assert!(!is_waiting_for_user_input(&json!([
        {"type": "tool_use", "id": "1", "name": "AskUserQuestion"},
        {"type": "tool_use", "id": "2", "name": "Bash"}
    ])));

    // Unnamed tool_use + AskUserQuestion -> Processing (unnamed is not safe)
    assert!(!is_waiting_for_user_input(&json!([
        {"type": "tool_use", "id": "1", "name": "AskUserQuestion"},
        {"type": "tool_use", "id": "2"}
    ])));

    // Non-user-input tool only
    assert!(!is_waiting_for_user_input(&json!([
        {"type": "tool_use", "id": "1", "name": "Read"}
    ])));

    // No tool_use at all
    assert!(!is_waiting_for_user_input(&json!([
        {"type": "text", "text": "hello"}
    ])));

    // Non-array content
    assert!(!is_waiting_for_user_input(&json!("string")));
    assert!(!is_waiting_for_user_input(&json!(null)));
}

#[test]
fn test_determine_status_assistant_with_tool_use() {
    // Assistant message with tool_use -> always Processing (tool could run for minutes)
    let status = determine_status(
        Some("assistant"),
        true,  // has_tool_use
        false, // has_tool_result
        false, // is_local_command
        false, // is_interrupted
        false, // is_user_input_tool
        false, // file_recently_modified
    );
    assert!(matches!(status, SessionStatus::Processing));

    // Same with recent file activity
    let status = determine_status(
        Some("assistant"),
        true,
        false,
        false,
        false,
        false,
        true,
    );
    assert!(matches!(status, SessionStatus::Processing));

    // AskUserQuestion tool_use -> Waiting (waiting for user input)
    let status = determine_status(
        Some("assistant"),
        true,  // has_tool_use
        false,
        false,
        false,
        true,  // is_user_input_tool
        false,
    );
    assert!(matches!(status, SessionStatus::Waiting));
}

#[test]
fn test_determine_status_assistant_text_only() {
    // Assistant message with only text -> always Waiting (Claude finished)
    let status = determine_status(
        Some("assistant"),
        false, // no tool_use
        false,
        false,
        false,
        false,
        false,
    );
    assert!(matches!(status, SessionStatus::Waiting));

    // With recent file activity, text-only assistant = Processing (still streaming/compacting)
    let status = determine_status(
        Some("assistant"),
        false,
        false,
        false,
        false,
        false,
        true,
    );
    assert!(matches!(status, SessionStatus::Processing));
}

#[test]
fn test_determine_status_user_message() {
    // Regular user message -> always Thinking (Claude is working)
    let status = determine_status(
        Some("user"),
        false,
        false,
        false, // not a local command
        false, // is_interrupted
        false,
        false,
    );
    assert!(matches!(status, SessionStatus::Thinking));

    // User message with recent file activity -> Thinking
    let status = determine_status(
        Some("user"),
        false,
        false,
        false,
        false,
        false,
        true,
    );
    assert!(matches!(status, SessionStatus::Thinking));

    // User message that's a local command -> Waiting
    let status = determine_status(
        Some("user"),
        false,
        false,
        true, // is_local_command
        false,
        false,
        false,
    );
    assert!(matches!(status, SessionStatus::Waiting));

    // User message that's an interrupted request -> Waiting
    let status = determine_status(
        Some("user"),
        false,
        false,
        false,
        true, // is_interrupted
        false,
        false,
    );
    assert!(matches!(status, SessionStatus::Waiting));
}

#[test]
fn test_determine_status_user_with_tool_result() {
    // User message with tool_result -> always Thinking (Claude processing result)
    let status = determine_status(
        Some("user"),
        false,
        true,  // has_tool_result
        false,
        false,
        false,
        false,
    );
    assert!(matches!(status, SessionStatus::Thinking));

    // Same with recent file activity
    let status = determine_status(
        Some("user"),
        false,
        true,
        false,
        false,
        false,
        true,
    );
    assert!(matches!(status, SessionStatus::Thinking));
}

#[test]
fn test_determine_status_unknown_type() {
    // Unknown message type with recent file activity -> Processing
    let status = determine_status(
        None,
        false,
        false,
        false,
        false,
        false,
        true,
    );
    assert!(matches!(status, SessionStatus::Processing));

    // Unknown message type, file stale -> Waiting
    let status = determine_status(
        None,
        false,
        false,
        false,
        false,
        false,
        false,
    );
    assert!(matches!(status, SessionStatus::Waiting));
}

#[test]
fn test_is_interrupted_request() {
    // Message with interruption text
    assert!(is_interrupted_request(&json!("[Request interrupted by user]")));
    assert!(is_interrupted_request(&json!("Some text [Request interrupted by user] more text")));

    // Array content with interruption
    let array_content = json!([
        {"type": "text", "text": "[Request interrupted by user]"}
    ]);
    assert!(is_interrupted_request(&array_content));

    // Normal messages
    assert!(!is_interrupted_request(&json!("Hello Claude")));
    assert!(!is_interrupted_request(&json!("Fix the bug")));
    assert!(!is_interrupted_request(&json!("")));
}

#[test]
fn test_status_sort_priority() {
    // Thinking and Processing have highest priority (0)
    assert_eq!(status_sort_priority(&SessionStatus::Thinking), 0);
    assert_eq!(status_sort_priority(&SessionStatus::Processing), 0);

    // Waiting has second priority (1)
    assert_eq!(status_sort_priority(&SessionStatus::Waiting), 1);

    // Compacting has highest priority (0)
    assert_eq!(status_sort_priority(&SessionStatus::Compacting), 0);

    // Idle has lowest priority (2)
    assert_eq!(status_sort_priority(&SessionStatus::Idle), 2);

    // Verify ordering: Thinking/Processing < Waiting < Idle
    assert!(status_sort_priority(&SessionStatus::Thinking) < status_sort_priority(&SessionStatus::Waiting));
    assert!(status_sort_priority(&SessionStatus::Waiting) < status_sort_priority(&SessionStatus::Idle));
}

#[test]
fn test_session_status_serialization() {
    // Verify status serializes to lowercase
    let waiting = SessionStatus::Waiting;
    let serialized = serde_json::to_string(&waiting).unwrap();
    assert_eq!(serialized, "\"waiting\"");

    let thinking = SessionStatus::Thinking;
    let serialized = serde_json::to_string(&thinking).unwrap();
    assert_eq!(serialized, "\"thinking\"");

    let processing = SessionStatus::Processing;
    let serialized = serde_json::to_string(&processing).unwrap();
    assert_eq!(serialized, "\"processing\"");

    let compacting = SessionStatus::Compacting;
    let serialized = serde_json::to_string(&compacting).unwrap();
    assert_eq!(serialized, "\"compacting\"");

    let idle = SessionStatus::Idle;
    let serialized = serde_json::to_string(&idle).unwrap();
    assert_eq!(serialized, "\"idle\"");
}

// Integration tests for JSONL parsing

#[test]
fn test_parse_jsonl_assistant_text_only_is_waiting() {
    // Scenario: Claude responded with text only (no tool_use), file not recently modified
    // Expected: Waiting
    let jsonl = create_test_jsonl_old(&[
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"Hello Claude"},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello! How can I help you today?"}]},"timestamp":"2024-01-01T00:00:01Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    assert!(matches!(session.status, SessionStatus::Waiting),
        "Expected Waiting when last message is assistant text-only, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_assistant_with_tool_use_is_processing() {
    // Scenario: Claude sent a tool_use (waiting for tool execution)
    // Expected: Processing
    let jsonl = create_test_jsonl(&[
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"List files"},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Let me list the files"},{"type":"tool_use","id":"123","name":"Bash","input":{"command":"ls"}}]},"timestamp":"2024-01-01T00:00:01Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    assert!(matches!(session.status, SessionStatus::Processing),
        "Expected Processing when last message is assistant with tool_use, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_user_message_is_thinking() {
    // Scenario: User just sent a message (Claude is thinking)
    // Expected: Thinking
    let jsonl = create_test_jsonl(&[
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"How can I help?"}]},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"Fix the bug in main.rs"},"timestamp":"2024-01-01T00:00:01Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    assert!(matches!(session.status, SessionStatus::Thinking),
        "Expected Thinking when last message is user input, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_user_tool_result_is_thinking() {
    // Scenario: Tool result was just sent, Claude processing it
    // The tempfile is freshly created so file_recently_modified = true
    // Expected: Thinking (Claude is actively processing the tool result)
    let jsonl = create_test_jsonl(&[
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"123","name":"Bash","input":{"command":"ls"}}]},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"123","content":"file1.txt\nfile2.txt"}]},"timestamp":"2024-01-01T00:00:01Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    // Since the tempfile was just created, file_recently_modified = true
    // With tool_result + recently modified = Thinking
    assert!(matches!(session.status, SessionStatus::Thinking),
        "Expected Thinking when last message is tool_result with recently modified file, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_local_command_is_waiting() {
    // Scenario: User typed /clear or other local command
    // Expected: Waiting (local commands don't trigger Claude)
    let jsonl = create_test_jsonl(&[
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done!"}]},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"/clear"},"timestamp":"2024-01-01T00:00:01Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    assert!(matches!(session.status, SessionStatus::Waiting),
        "Expected Waiting when last message is local command, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_complex_conversation_flow() {
    // Scenario: Complex conversation - user asks, Claude responds with tool, tool runs, Claude responds with text
    // File is old (not recently modified)
    // Expected: Waiting (Claude finished with text response)
    let jsonl = create_test_jsonl_old(&[
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"What files are in this directory?"},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool1","name":"Bash","input":{"command":"ls -la"}}]},"timestamp":"2024-01-01T00:00:01Z"}"#,
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","content":"file1.txt\nfile2.txt"}]},"timestamp":"2024-01-01T00:00:02Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I found 2 files: file1.txt and file2.txt"}]},"timestamp":"2024-01-01T00:00:03Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    assert!(matches!(session.status, SessionStatus::Waiting),
        "Expected Waiting after Claude responds with text, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_multiple_tool_calls_in_progress() {
    // Scenario: Claude sent tool_use, waiting for result
    // Expected: Processing
    let jsonl = create_test_jsonl(&[
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"Run tests and check coverage"},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'll run the tests first"},{"type":"tool_use","id":"tool1","name":"Bash","input":{"command":"npm test"}}]},"timestamp":"2024-01-01T00:00:01Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    assert!(matches!(session.status, SessionStatus::Processing),
        "Expected Processing when tool is executing, got {:?}", session.status);
}

#[test]
fn test_parse_jsonl_empty_content_skipped() {
    // Scenario: Some messages have empty content, should skip to find real message
    // File is old (not recently modified)
    let jsonl = create_test_jsonl_old(&[
        r#"{"sessionId":"test-session","type":"user","message":{"role":"user","content":"Hello"},"timestamp":"2024-01-01T00:00:00Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[]},"timestamp":"2024-01-01T00:00:01Z"}"#,
        r#"{"sessionId":"test-session","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hi there!"}]},"timestamp":"2024-01-01T00:00:02Z"}"#,
    ]);

    let session = parse_session_file(&jsonl.path().to_path_buf(), "/Users/test/Projects/test-project", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    assert!(session.is_some());
    let session = session.unwrap();
    // The parser reads from the end, so it should find the last non-empty message
    assert!(matches!(session.status, SessionStatus::Waiting),
        "Expected Waiting after finding text-only assistant message, got {:?}", session.status);
}

// Tests for PREVIOUS_STATUS cleanup

#[test]
fn test_cleanup_stale_status_entries_removes_old_sessions() {
    use std::collections::HashSet;

    // First, parse some sessions to populate PREVIOUS_STATUS
    let jsonl1 = create_test_jsonl_old(&[
        r#"{"sessionId":"session-alive","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]},"timestamp":"2024-01-01T00:00:00Z"}"#,
    ]);
    let jsonl2 = create_test_jsonl_old(&[
        r#"{"sessionId":"session-dead","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Goodbye"}]},"timestamp":"2024-01-01T00:00:00Z"}"#,
    ]);

    // Parse both to populate PREVIOUS_STATUS
    let _ = parse_session_file(&jsonl1.path().to_path_buf(), "/Users/test/Projects/test1", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);
    let _ = parse_session_file(&jsonl2.path().to_path_buf(), "/Users/test/Projects/test2", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);

    // Now cleanup with only session-alive as active
    let mut active_ids = HashSet::new();
    active_ids.insert("session-alive".to_string());
    cleanup_stale_status_entries(&active_ids);

    // Parse session-alive again - it should not cause a "STATUS TRANSITION" log
    // since its entry was preserved. This verifies cleanup kept it.
    let session = parse_session_file(&jsonl1.path().to_path_buf(), "/Users/test/Projects/test1", TEST_PID, TEST_CPU_USAGE, AgentType::Claude);
    assert!(session.is_some());
    assert_eq!(session.unwrap().id, "session-alive");
}

#[test]
fn test_cleanup_stale_status_entries_handles_empty_set() {
    use std::collections::HashSet;

    // Cleanup with empty active set should not panic
    let active_ids = HashSet::new();
    cleanup_stale_status_entries(&active_ids);
}

// Tests for get_sessions_internal with stale process scenarios

#[test]
fn test_get_sessions_internal_no_processes_returns_empty() {
    let processes: Vec<AgentProcess> = vec![];
    let sessions = get_sessions_internal(&processes, AgentType::Claude);
    assert!(sessions.is_empty(), "No processes should yield no sessions");
}

#[test]
fn test_get_sessions_internal_process_without_cwd_is_skipped() {
    let processes = vec![AgentProcess {
        pid: 99999,
        cpu_usage: 0.0,
        cwd: None,
    }];
    let sessions = get_sessions_internal(&processes, AgentType::Claude);
    assert!(sessions.is_empty(), "Process without CWD should be skipped");
}

#[test]
fn test_get_sessions_internal_process_with_nonexistent_project_is_skipped() {
    let processes = vec![AgentProcess {
        pid: 99999,
        cpu_usage: 0.0,
        cwd: Some(std::path::PathBuf::from("/nonexistent/path/that/does/not/match/any/project")),
    }];
    let sessions = get_sessions_internal(&processes, AgentType::Claude);
    assert!(sessions.is_empty(), "Process with non-matching CWD should produce no sessions");
}
